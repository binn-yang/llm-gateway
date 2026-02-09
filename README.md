# LLM Gateway

A high-performance LLM proxy gateway written in Rust that provides multiple API formats for LLM providers (OpenAI, Anthropic Claude, Google Gemini):
- **Unified OpenAI-compatible API** (`/v1/chat/completions`) - works with all providers via automatic protocol conversion
- **Native Anthropic Messages API** (`/v1/messages`) - direct passthrough for Claude models without conversion overhead
- **Advanced Failover System** (NEW) - Circuit breaker, exponential backoff, intelligent error classification
- **SQLite-based Observability** - Complete request logging with token tracking and performance metrics
- **Web Dashboard** - Real-time monitoring and analytics UI built with Vue 3

## Features

- **Multiple API Formats**:
  - Unified OpenAI-compatible API (`/v1/chat/completions`) with automatic protocol conversion
  - Native Anthropic Messages API (`/v1/messages`) for direct Claude access
- **Protocol Conversion**: Automatic request/response translation between OpenAI, Anthropic, and Gemini formats
- **Smart Routing**: Prefix-based model routing to appropriate providers
- **Multi-Instance Load Balancing**: Each provider supports multiple backend instances with priority-based selection
- **Sticky Sessions**: API key-level session affinity maximizes provider-side KV cache hits
- **Advanced Failover System** (NEW in v0.5.0):
  - **Circuit Breaker**: 3 failures trigger circuit open, half-open state for testing recovery
  - **Intelligent Error Classification**: 401/403 auth errors, 429 rate limits, 503 transient errors handled differently
  - **Exponential Backoff**: 60s → 120s → 240s → 480s → 600s with ±20% jitter
  - **Automatic Retry**: Smart retry logic with max 3 attempts, different strategies per error type
  - **Health Monitoring**: Real-time health status via `stats` command (✅ Healthy / 🟡 Recovering / 🔴 Unhealthy)
  - **Event Logging**: failover_events table tracks all circuit breaker state transitions
- **SQLite-based Observability**:
  - Complete request logging with token usage tracking
  - Anthropic prompt caching metrics (cache creation/read tokens)
  - **Automatic Cost Calculation** (NEW):
    - Real-time cost tracking for all requests (streaming and non-streaming)
    - Supports input/output/cache tokens cost breakdown
    - Hourly pricing data updates from remote source
    - Per-request cost stored in database for analytics
  - Automatic data retention policies (7-30 days)
  - Non-blocking async batch writes
  - **Provider Quota Monitoring**:
    - Automatic quota refresh for Anthropic OAuth instances
    - Real-time quota status via CLI stats command
    - Supports 5-hour, 7-day, and 7-day (Sonnet) usage windows
- **Web Dashboard** (NEW):
  - Real-time token usage charts and analytics
  - Provider instance health monitoring
  - Per-API-key cost estimation
  - Request trace visualization
  - **Configuration Management UI** - CRUD operations for API keys, routing rules, and provider instances
- **Flexible Configuration**:
  - Database-driven configuration with hot reload (no server restart required)
  - Web UI for managing API keys, routing rules, and provider instances
  - TOML file support for backward compatibility and initial setup
  - Dual authentication: Gateway API keys (SHA256 hashed) + Provider API keys (encrypted storage)
- **SQLite-based Metrics**: Unified observability with per-request granularity and automatic retention
- **Streaming Support**: Full SSE support with real-time protocol conversion
- **Cloud Native**: Docker ready, health checks, structured JSON logging
- **Horizontal Scaling**: Nginx-compatible for multi-machine deployments

## Architecture

The gateway provides two API formats:

```
┌─────────────────────────────────────────────────────────────────┐
│  Option 1: OpenAI-compatible API (all providers)                │
│  ┌─────────────┐                                                │
│  │   Cursor    │                                                │
│  │  Continue   │  → /v1/chat/completions → Gateway →           │
│  │   etc.      │                          Auto-routes to:       │
│  └─────────────┘                          ├─ OpenAI (direct)   │
│                                            ├─ Anthropic (convert)│
│                                            └─ Gemini (convert)  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Option 2: Native Anthropic API (Claude only)                   │
│  ┌─────────────┐                                                │
│  │ Claude Code │  → /v1/messages → Gateway → Anthropic          │
│  │  Anthropic  │                   (native format, no convert)  │
│  │    SDK      │                                                │
│  └─────────────┘                                                │
└─────────────────────────────────────────────────────────────────┘
```

## Load Balancing & High Availability

### Multi-Provider Instance Architecture

Each provider type (OpenAI, Anthropic, Gemini) can have **multiple backend instances** for load balancing and automatic failover:

```
┌──────────────────────────────────────────────────────────┐
│  Client Request (API Key = "sk-user-alice")              │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────┐
│  Gateway: LoadBalancer (Priority-Based Sticky Sessions)  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  SessionMap (API Key → Instance Binding)           │  │
│  │  - "sk-user-alice" → "anthropic-primary"           │  │
│  │  - Session TTL: 1 hour (auto-refresh on request)   │  │
│  └────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  HealthState (Instance → Health Status)            │  │
│  │  - "anthropic-primary": healthy                    │  │
│  │  - "anthropic-backup": healthy                     │  │
│  └────────────────────────────────────────────────────┘  │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
        ┌────────────┴────────────┐
        │                         │
        ▼                         ▼
┌──────────────┐         ┌──────────────┐
│  Primary     │         │  Backup      │
│  Instance    │         │  Instance    │
│  priority=1  │         │  priority=2  │
└──────────────┘         └──────────────┘
```

### Sticky Session Strategy

**Why Sticky Sessions?**
- **Maximizes KV Cache Hits**: Same user → same instance → provider can reuse conversation context
- **Minimal Lock Contention**: DashMap with segment locking + RwLock for read-heavy health checks
- **Predictable Performance**: No random load distribution that breaks cache locality

**How It Works:**
1. **First Request**: User makes initial request → LoadBalancer selects instance by priority
2. **Session Creation**: API key bound to selected instance for 1 hour
3. **Subsequent Requests**: Same API key always routes to same instance (until failure or timeout)
4. **Session Expiry**: After 1 hour of inactivity, session expires → next request reselects by priority

### Priority-Based Selection

Instances are configured with a **priority** value (lower number = higher priority):

```toml
# Primary instance (always preferred when healthy)
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1                    # Highest priority

# Backup instance (used only when primary fails)
[[providers.anthropic]]
name = "anthropic-backup"
priority = 2                    # Lower priority

# Another backup (same priority = random selection)
[[providers.anthropic]]
name = "anthropic-backup-2"
priority = 2                    # Same priority → random among these two
```

**Selection Algorithm:**
1. Filter: Only healthy and enabled instances
2. Find minimum priority value among healthy instances
3. Random selection among instances with that priority
4. Bind API key to selected instance (sticky session)

### Automatic Failover & Recovery

#### Health Detection Criteria

An instance is marked **unhealthy** based on intelligent error classification:

| Failure Type | Examples | Action | Retry Strategy |
|--------------|----------|--------|----------------|
| **Authentication Errors** | 401, 403 | Mark unhealthy (config issue) | Switch to backup |
| **Rate Limit** | 429 | Delay + retry | Wait retry_after seconds |
| **Transient Errors** | 503 Service Unavailable | **No marking** | Immediate retry on different instance |
| **Server Errors** | 500, 502, 504 | Mark unhealthy | Circuit breaker + failover |
| **Connection Failures** | TCP timeout, DNS failure | Mark unhealthy | Circuit breaker + failover |
| **Request Timeouts** | Exceeds timeout_seconds | Mark unhealthy | Circuit breaker + failover |
| **Business Errors** | Invalid model, bad request | **No action** | Return to client |

#### Circuit Breaker Pattern (NEW in v0.5.0)

The gateway implements a sophisticated circuit breaker to prevent cascading failures:

**Three States:**
```
Closed (Normal Operation)
    ↓ (3 failures in 60s)
Open (Blocking Requests)
    ↓ (After backoff period)
Half-Open (Testing Recovery)
    ↓ (2 consecutive successes)
Closed (Back to Normal)
```

**Configuration** (hardcoded defaults):
- Failure threshold: 3 failures within 60 seconds
- Success threshold: 2 consecutive successes to close circuit
- Max retries per request: 3 attempts

**Example Timeline:**
```
T+0s:    Request fails (1/3)
         ⚠️ Failure recorded

T+5s:    Request fails (2/3)
         ⚠️ Failure recorded

T+10s:   Request fails (3/3)
         🔴 Circuit opens
         → Requests blocked for 60s

T+70s:   Health check passes
         🟡 Circuit half-open
         → Testing with real traffic

T+75s:   Request succeeds (1/2)
T+80s:   Request succeeds (2/2)
         ✅ Circuit closed
         → Normal operation resumed
```

#### Exponential Backoff Recovery

Instead of fixed 60-second recovery, the gateway uses **exponential backoff with jitter**:

| Attempt | Base Backoff | With ±20% Jitter | Max |
|---------|--------------|------------------|-----|
| 1st failure | 60s | 48-72s | - |
| 2nd failure | 120s | 96-144s | - |
| 3rd failure | 240s | 192-288s | - |
| 4th failure | 480s | 384-576s | - |
| 5th+ failure | 600s | 480-720s | 10 min cap |

**Benefits:**
- Prevents premature recovery attempts
- Reduces load on failing instances
- Jitter prevents thundering herd
- Adaptive to failure duration

#### Retry Logic by Error Type

**Rate Limit (429):**
```
1. Parse retry_after header (default 2s)
2. Mark instance unhealthy
3. Wait retry_after seconds
4. Retry with different instance
5. Max 3 total attempts
```

**Transient Error (503):**
```
1. Do NOT mark instance unhealthy
2. Immediately retry with different instance
3. Max 3 total attempts
```

**Instance Failure (5xx, timeout):**
```
1. Mark instance unhealthy
2. Trigger circuit breaker logic
3. Immediately retry with backup instance
4. Max 3 total attempts
```

**Business Error (4xx except 429):**
```
1. Do NOT retry
2. Return error to client immediately
```

#### Auto-Recovery Mechanism

**Passive Time-Based Recovery** (no active health probes):

```
Timeline Example:

T+0s:    Request succeeds on primary instance
         ✓ Session: sk-user-alice → primary

T+30s:   Request fails on primary (502 Bad Gateway)
         ✗ Primary marked unhealthy
         ✓ Session unchanged (fails this request)

T+35s:   Next request detects primary unhealthy
         → Session deleted
         → Selects backup instance (priority=2)
         ✓ New session: sk-user-alice → backup

T+90s:   Primary auto-recovers (60s timeout passed)
         ✓ Primary marked healthy again
         ✓ User still on backup (session active)

T+3635s: Session expires (1 hour since last request)
         → Next request reselects by priority
         ✓ Returns to primary (priority=1)
```

**Recovery Configuration:**

```toml
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1
failure_timeout_seconds = 60    # Auto-recover after 60s
```

#### Gradual Recovery (Anti-Flapping)

The system implements **gradual recovery** to prevent "flapping" (rapid switching):

1. **Immediate Failover**: Instance failure → immediate switch to backup
2. **Delayed Return**: Instance recovery → users gradually return via session expiry
3. **No Forced Migration**: Existing sessions stay on backup until natural expiry
4. **Progressive Load**: New sessions go to primary, old sessions stay on backup

### Horizontal Scaling with Nginx

For multi-machine deployments, use **Nginx with consistent hashing** to add a second layer of stickiness:

```nginx
# nginx.conf
upstream llm_gateway_cluster {
    # Consistent hashing on Authorization header (API key)
    hash $http_authorization consistent;

    server gateway-1.internal:8080;
    server gateway-2.internal:8080;
    server gateway-3.internal:8080;
}

server {
    listen 80;
    server_name api.example.com;

    location / {
        proxy_pass http://llm_gateway_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header Authorization $http_authorization;

        # LLM requests can be long-running
        proxy_read_timeout 300s;
        proxy_connect_timeout 10s;
    }
}
```

**Two-Layer Sticky Architecture:**

```
Client (sk-user-alice)
    │
    ▼
Nginx Layer 1: hash(API key) → Gateway-2
    │
    ▼
Gateway-2 Layer 2: session(API key) → Anthropic-Primary
    │
    ▼
Provider Instance (KV Cache Hit!)
```

**Benefits:**
- ✅ Fully stateless gateways (no cross-process communication)
- ✅ No Redis/shared state required
- ✅ Extreme performance (two memory-only hash lookups)
- ✅ Easy scaling (just add/remove gateway instances in Nginx upstream)
- ✅ Fault isolation (one gateway failure doesn't affect others)

## Quick Start

### 1. Configuration

**Important:** Never commit `config.toml` with real API keys to version control!

Create your configuration file from the example:

```bash
cp config.toml.example config.toml
```

Then edit `config.toml` and replace the placeholder values with your actual API keys:

```toml
[server]
host = "0.0.0.0"
port = 8080
log_level = "info"
log_format = "json"

# API Keys
[[api_keys]]
key = "sk-gateway-001"
name = "my-app"
enabled = true

# Model Mapping (defines which provider each model uses)
[models.gpt-4]
provider = "openai"
api_model = "gpt-4"

[models."claude-3-5-sonnet"]
provider = "anthropic"
api_model = "claude-3-5-sonnet-20241022"

[models."gemini-1.5-pro"]
provider = "gemini"
api_model = "models/gemini-1.5-pro-latest"

# Provider Configurations
[providers.openai]
enabled = true
api_key = "sk-your-openai-key"
base_url = "https://api.openai.com/v1"
timeout_seconds = 300

[providers.anthropic]
enabled = true
api_key = "sk-ant-your-anthropic-key"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"

# Or use OAuth authentication (enables quota monitoring)
[[providers.anthropic]]
name = "anthropic-oauth"
enabled = true
auth_mode = "oauth"
oauth_provider = "anthropic"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"

[providers.gemini]
enabled = true
api_key = "your-gemini-key"
base_url = "https://generativelanguage.googleapis.com/v1beta"
timeout_seconds = 300

# Metrics
[metrics]
enabled = true
endpoint = "/metrics"
include_api_key_hash = true
```

### 2. Run with Docker

```bash
docker build -t llm-gateway .
docker run -p 8080:8080 -v $(pwd)/config.toml:/app/config.toml llm-gateway
```

### 3. Run from source

```bash
# Backend only
cd backend
cargo run --release

# With frontend (for development)
cd frontend
npm install
npm run dev        # Frontend dev server on http://localhost:3000

# Production build (frontend)
cd frontend
npm run build      # Builds to frontend/dist/
cd ../backend
cargo run --release  # Serves frontend from /
```

### 4. Access the Dashboard

Once running, access the web dashboard at:
```
http://localhost:8080/
```

The dashboard provides:
- Real-time token usage monitoring
- Provider instance health status
- Per-API-key analytics and cost estimation
- Request trace visualization

## API Endpoints

### Core LLM APIs

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/chat/completions` | POST | Yes | OpenAI-compatible chat completion (all providers) |
| `/v1/messages` | POST | Yes | Native Anthropic Messages API (Claude models only) |
| `/v1/models` | GET | Yes | List available models |

### Monitoring & Observability

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/health` | GET | No | Health check |
| `/ready` | GET | No | Readiness check |

### Dashboard APIs (NEW)

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/` | GET | No | Web Dashboard (Vue 3 SPA) |
| `/api/requests/time-series` | GET | No | Token usage time series data |
| `/api/requests/by-api-key` | GET | No | Per-API-key token aggregation |
| `/api/requests/by-instance` | GET | No | Per-instance token distribution |
| `/api/instances/health-time-series` | GET | No | Instance health over time |
| `/api/instances/current-health` | GET | No | Current instance health status |

### Configuration Management APIs (NEW)

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/api/config/api-keys` | GET | No | List all API keys |
| `/api/config/api-keys` | POST | No | Create new API key |
| `/api/config/api-keys/:name` | PUT | No | Update API key (enable/disable) |
| `/api/config/api-keys/:name` | DELETE | No | Delete API key |
| `/api/config/routing-rules` | GET | No | List all routing rules |
| `/api/config/routing-rules` | POST | No | Create new routing rule |
| `/api/config/routing-rules/:id` | PUT | No | Update routing rule |
| `/api/config/routing-rules/:id` | DELETE | No | Delete routing rule |
| `/api/config/providers/:provider/instances` | GET | No | List provider instances |
| `/api/config/providers/:provider/instances` | POST | No | Create provider instance |
| `/api/config/providers/:provider/instances/:name` | PUT | No | Update provider instance |
| `/api/config/providers/:provider/instances/:name` | DELETE | No | Delete provider instance |
| `/api/config/reload` | POST | No | Reload configuration from database |

## Configuration Management

### Web-Based Configuration UI

Access the configuration management interface at `http://localhost:8080/config` to manage your gateway settings through a user-friendly web interface.

**Features**:
- **API Keys Management**: Create, enable/disable, and delete gateway API keys
- **Routing Rules**: Configure model prefix-to-provider routing (e.g., "gpt-" → openai)
- **Provider Instances**: Manage multiple backend instances per provider with priority settings
- **Hot Reload**: Changes take effect immediately without server restart
- **Anthropic-Specific Settings**: Configure prompt caching and API version per instance

**Configuration Flow**:
```
1. Initial Setup (TOML file)
   ↓
2. Server loads config into SQLite database
   ↓
3. Use Web UI to manage configuration
   ↓
4. Changes saved to database + hot reload
   ↓
5. No server restart required!
```

**Important Notes**:
- **First Run**: Server loads configuration from `config.toml` into SQLite database
- **Subsequent Runs**: Configuration loaded from database (TOML file ignored unless database is empty)
- **API Key Storage**:
  - Gateway API keys: SHA256 hashed for authentication
  - Provider API keys: Stored as plaintext (required for upstream API calls)
- **Backup**: Database file is at `./data/config.db` - back it up regularly

### TOML Configuration (Legacy/Initial Setup)

For initial setup or automated deployments, you can still use `config.toml`:

## Usage Examples

### Using with Cursor

```bash
export OPENAI_API_BASE="http://localhost:8080/v1"
export OPENAI_API_KEY="sk-gateway-001"

# Now Cursor can use any model configured in the gateway
# Just change the model name in Cursor's settings:
# - "gpt-4" → OpenAI
# - "claude-3-5-sonnet" → Anthropic (via conversion)
# - "gemini-1.5-pro" → Gemini (via conversion)
```

### Using with Claude Code

```bash
# Native Anthropic API (recommended for Claude Code):
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_API_KEY="sk-gateway-001"

# Claude Code will use /v1/messages endpoint with native Anthropic format
```

### CLI Commands

The gateway provides several CLI commands for management and monitoring:

```bash
# Start the server
./target/release/llm-gateway start

# Start as daemon (background process)
./target/release/llm-gateway start --daemon

# Stop the daemon
./target/release/llm-gateway stop

# Reload configuration (hot reload without restart)
./target/release/llm-gateway reload

# View system statistics and quota status
./target/release/llm-gateway stats [--hours HOURS] [--detailed]

# OAuth management
./target/release/llm-gateway oauth login <provider>
./target/release/llm-gateway oauth status <provider>
./target/release/llm-gateway oauth refresh <provider>
./target/release/llm-gateway oauth logout <provider>

# Test configuration
./target/release/llm-gateway test

# Show version
./target/release/llm-gateway version
```

**Stats Command Examples**:

```bash
# View statistics for last 24 hours
./target/release/llm-gateway stats

# View statistics for last 7 days with detailed output
./target/release/llm-gateway stats --hours 168 --detailed
```

**Stats Output Includes**:
- System Summary: API keys, providers, healthy instances
- **Provider Health Status** (NEW): Real-time circuit breaker state and failure counts
- Token Usage: Per-model breakdown with cache metrics
- Quota Status: Provider instance quota utilization (OAuth only)
- Database Stats: Request counts, uptime, active keys

**Example Stats Output:**
```
Provider Health Status:
  openai-primary                 ✅ Healthy      (0 failures)
  anthropic-primary              🟡 Recovering   (testing recovery, retry in 45s)
  anthropic-backup               ✅ Healthy      (0 failures)
  gemini-main                    🔴 Unhealthy    (5 failures, retry in 8m)

Overall: 2/4 healthy, 1 recovering, 1 down
```

### Direct API Calls

**Option 1: OpenAI-compatible API** (works with all providers)

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [
      {"role": "user", "content": "Hello!"}
    ]
  }'
```

**Option 2: Native Anthropic API** (Claude only, no conversion)

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "Hello!"}
    ]
  }'
```

## Observability & Dashboard

### Web Dashboard

Access the dashboard at `http://localhost:8080/` to monitor your gateway in real-time:

**Features**:
- **Token Usage Analytics**: Visualize token consumption over time with interactive charts
- **Cost Estimation**: Calculate costs based on token usage and prompt caching
- **Provider Health**: Monitor instance health status and failover events
- **API Key Breakdown**: Per-key token usage and cost analysis
- **Request Traces**: Visualize request traces with performance breakdown

**Technology**:
- Built with Vue 3 + TypeScript + Chart.js
- Real-time data from SQLite database
- Responsive design with Tailwind CSS

### SQLite-based Observability

All requests are logged to SQLite database (`./data/observability.db`) with complete details:

**Request Data Includes**:
- Basic info: request_id, timestamp, api_key_name, provider, instance, model, endpoint
- Token usage: input_tokens, output_tokens, total_tokens
- **Caching metrics**: cache_creation_input_tokens, cache_read_input_tokens (Anthropic only)
- **Cost breakdown**: input_cost, output_cost, cache_write_cost, cache_read_cost, total_cost
- Performance: duration_ms, status, error_type, error_message

**Failover Event Tracking** (NEW in v0.5.0):

The gateway tracks all circuit breaker events in the `failover_events` table:

**Event Types:**
- `failure` - Instance failure recorded
- `circuit_open` - Circuit breaker opened (3 failures)
- `circuit_half_open` - Testing recovery (health check passed)
- `circuit_closed` - Circuit closed (2 successes)
- `recovery` - Instance recovered

**Query Examples:**
```sql
-- Recent failover events
SELECT datetime(timestamp) as time,
       provider, instance, event_type,
       consecutive_failures, next_retry_secs
FROM failover_events
ORDER BY timestamp DESC
LIMIT 20;

-- Circuit breaker state by instance
SELECT instance,
       event_type,
       consecutive_failures,
       datetime(timestamp) as last_event
FROM failover_events
WHERE (provider, instance, timestamp) IN (
    SELECT provider, instance, MAX(timestamp)
    FROM failover_events
    GROUP BY provider, instance
)
ORDER BY provider, instance;
```

**Provider Quota Monitoring** (NEW):

The gateway automatically monitors token quotas for provider instances:

**Supported Providers**:
- ✅ **Anthropic (OAuth mode only)**: Supports quota queries via Anthropic's OAuth usage API
- ❌ **Anthropic (Bearer mode)**: Not supported (no public API available)
- ❌ **OpenAI**: Not supported (no public API available)
- ❌ **Gemini**: Not supported (no public API available)

**Why OAuth Only?**

Anthropic provides a dedicated usage API endpoint (`https://api.anthropic.com/api/oauth/usage`) that requires:
- OAuth Bearer token authentication
- Special beta header: `anthropic-beta: oauth-2025-04-20`

**Bearer mode limitation**: API Key authentication does not have access to quota query APIs. To monitor quotas for Anthropic, you must use OAuth authentication mode.

**Quota Data Collected** (Anthropic OAuth):
- 5-hour usage window utilization
- 7-day usage window utilization
- 7-day Sonnet-specific usage window utilization
- Reset timestamps for each window

**Automatic Refresh**:
- Background task queries provider APIs every 10 minutes (configurable)
- Quota snapshots stored in SQLite database
- Automatic cleanup of old snapshots (7-day retention)

**View Quota Status**:

```bash
# View current quota status for all provider instances
./target/release/llm-gateway stats
```

**Example Output**:
```
Quota Status:
╔══════════╦═══════════════════╦═══════════╦════════╦════════════════════════════════════╦═════════════╗
║ PROVIDER ║ INSTANCE          ║ AUTH MODE ║ STATUS ║ QUOTA INFO                         ║ LAST UPDATE ║
╠══════════╬═══════════════════╬═══════════╬════════╬════════════════════════════════════╬═════════════╣
║ anthropic║ anthropic-oauth   ║ oauth     ║ ✓ OK   ║ 5h: 35.0% | 7d: 42.0% | 7d(s): 50% ║ 2m ago      ║
║ anthropic║ anthropic-key     ║ bearer    ║ - N/A  ║ -                                  ║ 5m ago      ║
╚══════════╩═══════════════════╩═══════════╩════════╩════════════════════════════════════╩═════════════╝
```

**Status Codes**:
- ✓ OK: Quota data successfully retrieved
- ✗ ERROR: Failed to query quota (check logs for details)
- - N/A: Quota monitoring not available (bearer mode or unsupported provider)

**Configuration**:

```toml
[observability]
enabled = true
database_path = "./data/observability.db"

# Quota refresh configuration
[observability.quota_refresh]
enabled = true              # Enable/disable quota monitoring
interval_seconds = 600      # Refresh interval (default: 10 minutes)
timeout_seconds = 30        # Query timeout (default: 30 seconds)
retention_days = 7          # Data retention (default: 7 days)
```

**Query Examples**:
```sql
-- Token usage by provider (last 7 days)
SELECT provider, model,
       SUM(input_tokens) as total_input,
       SUM(output_tokens) as total_output,
       SUM(cache_read_input_tokens) as cache_savings
FROM requests
WHERE date >= date('now', '-7 days')
GROUP BY provider, model;

-- Cost analysis by model (last 7 days)
SELECT model,
       COUNT(*) as request_count,
       SUM(input_tokens) as total_input_tokens,
       SUM(output_tokens) as total_output_tokens,
       ROUND(SUM(total_cost), 4) as total_cost_usd,
       ROUND(AVG(total_cost), 6) as avg_cost_per_request
FROM requests
WHERE date >= date('now', '-7 days')
GROUP BY model
ORDER BY total_cost_usd DESC;

-- Slowest requests (p99 latency)
SELECT request_id, model, duration_ms, timestamp
FROM requests
ORDER BY duration_ms DESC
LIMIT 100;

-- Cache efficiency (Anthropic only)
SELECT
    COUNT(*) as requests,
    SUM(cache_read_input_tokens) as total_cached,
    SUM(input_tokens) as total_input,
    ROUND(100.0 * SUM(cache_read_input_tokens) / SUM(input_tokens), 2) as cache_hit_rate
FROM requests
WHERE provider = 'anthropic' AND date >= date('now', '-1 day');

-- Latest quota snapshots
SELECT provider, instance, auth_mode, status,
       datetime(timestamp/1000, 'unixepoch') as updated_at
FROM quota_snapshots
WHERE (provider, instance, timestamp) IN (
    SELECT provider, instance, MAX(timestamp)
    FROM quota_snapshots
    GROUP BY provider, instance
)
ORDER BY provider, instance;
```

**Data Retention**:
- Request logs: 7 days (configurable)
- Trace spans: 7 days
- Quota snapshots: 7 days (configurable)
- Automatic cleanup runs daily at 3 AM

All metrics are stored in SQLite and accessible via:
- **Web Dashboard**: Real-time charts at `http://localhost:8080/`
- **SQL Queries**: Direct database access for custom analytics
- **REST API**: Dashboard API endpoints for programmatic access
- **CLI Stats**: `./target/release/llm-gateway stats` for system overview

## Feature Matrix

The gateway supports comprehensive multimodal features across all providers:

| Feature | OpenAI | Anthropic | Gemini | Notes |
|---------|:------:|:---------:|:------:|-------|
| **Text Completion** | ✅ | ✅ | ✅ | Full support |
| **Streaming** | ✅ | ✅ | ✅ | SSE with real-time conversion |
| **Vision/Images** | ✅ | ✅ | ✅ | Automatic base64 conversion |
| **Tool Calling (Non-Streaming)** | ✅ | ✅ | ✅ | Full request/response conversion |
| **Tool Calling (Streaming)** | ✅ | ✅ | ✅ | Incremental JSON assembly |
| **Prompt Caching** | ❌ | ✅ | ❌ | Auto-caching for system prompts & tools |
| **JSON Mode** | ✅ | ✅ ⚠️ | ✅ | ⚠️ = System prompt injection workaround |
| **JSON Schema** | ✅ | ✅ ⚠️ | ✅ | ⚠️ = System prompt injection workaround |
| **Conversion Warnings** | N/A | ✅ | ✅ | X-LLM-Gateway-Warnings header |
| **Quota Monitoring (OAuth)** | ❌ | ✅ | ❌ | Anthropic OAuth only |

**Legend:**
- ✅ = Full native or converted support
- ⚠️ = Workaround via system prompt injection
- ❌ = Not supported by provider

### Vision/Image Support

Send images using OpenAI's format (works with all providers):

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "What's in this image?"},
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,...",
            "detail": "high"
          }
        }
      ]
    }
  ]
}
```

The gateway automatically:
- Converts base64 data URLs for all providers
- Handles multiple images in a single request
- Preserves image detail settings

### Tool/Function Calling

Define tools using OpenAI's format:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{"role": "user", "content": "What's the weather?"}],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get current weather",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {"type": "string"}
          },
          "required": ["location"]
        }
      }
    }
  ],
  "tool_choice": "auto"
}
```

The gateway converts to:
- **Anthropic**: `tools` array with `name`, `description`, `input_schema`
- **Gemini**: `function_declarations` with parameters schema

Supports:
- Auto tool selection
- Required tool use
- Specific tool forcing
- Multi-turn conversations with tool results
- Streaming tool calls with incremental JSON

### Prompt Caching (Anthropic)

Configure auto-caching in `config.toml`:

```toml
[[providers.anthropic]]
name = "anthropic-primary"
# ... other config ...

[providers.anthropic.cache]
auto_cache_system = true         # Auto-cache large system prompts
min_system_tokens = 1024          # Minimum tokens to trigger caching
auto_cache_tools = true           # Auto-cache tool definitions
```

The gateway automatically:
- Detects large system prompts (≥1024 tokens)
- Adds `cache_control` to last system prompt block
- Caches tool definitions (marked on last tool)
- Converts Text → Blocks format when needed

**Cost savings**: ~90% reduction on cached content!

### JSON Mode & Structured Outputs

Request JSON responses:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{"role": "user", "content": "List 3 colors"}],
  "response_format": {"type": "json_object"}
}
```

With strict schema:

```json
{
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "color_list",
      "strict": true,
      "schema": {
        "type": "object",
        "properties": {
          "colors": {
            "type": "array",
            "items": {"type": "string"}
          }
        },
        "required": ["colors"]
      }
    }
  }
}
```

**Provider implementation:**
- **OpenAI**: Native `response_format` support
- **Gemini**: Native via `response_mime_type` and `response_schema`
- **Anthropic**: System prompt injection (check `X-LLM-Gateway-Warnings` header)

### Conversion Warnings

When parameters aren't natively supported, the gateway adds warnings via HTTP header:

```http
X-LLM-Gateway-Warnings: [{"level":"warning","message":"Parameter 'seed' not supported by Anthropic provider, ignoring"}]
```

Warnings appear for:
- Unsupported parameters (`seed`, `logprobs`, `logit_bias`, etc.)
- Provider-specific workarounds (JSON mode on Anthropic)
- Feature limitations

## Protocol Conversion

The gateway automatically converts between protocols:

| Feature | OpenAI | Anthropic | Gemini | Conversion |
|---------|--------|-----------|--------|------------|
| System message | `messages[0].role="system"` | `system` field | `systemInstruction` | ✅ Extracted |
| Role names | `assistant` | `assistant` | `model` | ✅ Mapped |
| max_tokens | Optional | Required | Optional | ✅ Default: 4096 |
| temperature | 0-2 | 0-1 | 0-2 | ✅ Clipped |
| Content blocks | String or array | String or array | Parts array | ✅ Converted |
| Tools | OpenAI format | Anthropic format | Function declarations | ✅ Converted |
| Images | URL or base64 | Base64 only | Base64 only | ✅ Auto-converted |

## Examples

The repository includes comprehensive examples demonstrating all major features:

```bash
# Vision/image support
cargo run --example vision_example

# Tool/function calling
cargo run --example tool_calling_example

# JSON mode and structured outputs
cargo run --example json_mode_example

# Prompt caching for cost optimization
cargo run --example caching_example
```

Each example includes:
- Working code with detailed comments
- Multiple use cases per feature
- Provider-specific notes
- Cost optimization strategies

## Development

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# Specific feature tests
cargo test --test multimodal_tests
cargo test --test tool_calling_tests
cargo test --test json_mode_tests
cargo test --test caching_tests
```

### Building Release Binary

#### Quick Start (macOS Development)

```bash
# Fast development builds (debug profile, ~30s-1min)
cd backend
cargo build
cargo run

# macOS production build
cargo build --release
# Output: backend/target/release/llm-gateway
```

#### Cross-Platform Compilation (Linux)

The project supports cross-compilation to build Linux binaries from macOS.

**First-time setup:**

```bash
# 1. Install Linux target
rustup target add x86_64-unknown-linux-gnu

# 2. Install cross tool (requires Docker)
cargo install cross --git https://github.com/cross-rs/cross

# 3. Done! Now you can build for Linux
```

**Building Linux binaries:**

```bash
# Option 1: Use the build script (recommended)
./scripts/build-linux.sh

# Option 2: Direct command (must run from project root!)
cross build \
    --manifest-path backend/Cargo.toml \
    --target x86_64-unknown-linux-gnu \
    --release
# Output: backend/target/x86_64-unknown-linux-gnu/release/llm-gateway

# Option 3: Fully static Linux binary (no system dependencies)
cross build \
    --manifest-path backend/Cargo.toml \
    --target x86_64-unknown-linux-musl \
    --release
# Output: backend/target/x86_64-unknown-linux-musl/release/llm-gateway
```

**Important:** When using `cross` directly, always run it from the **project root directory** (not the `backend` directory), and use `--manifest-path backend/Cargo.toml`. This ensures that `frontend/dist` is accessible to the build container for embedding.

**Binary sizes:**
- macOS (release): ~10MB
- Linux (release): ~10MB
- Linux MUSL (static): ~12MB

**Troubleshooting:**

If you encounter OpenSSL-related errors, ensure you're using the latest code which has switched from `native-tls` to `rustls` (pure Rust SSL implementation).

For more documentation:
- **[IMPLEMENTATION.md](docs/IMPLEMENTATION.md)** - Complete implementation details and architecture
- **[FEATURES.md](docs/FEATURES.md)** - Comprehensive feature documentation
- **[CONVERSION_LIMITATIONS.md](docs/CONVERSION_LIMITATIONS.md)** - Provider conversion trade-offs
- **[DAEMON.md](docs/DAEMON.md)** - Running as a daemon/background service

## Configuration

### Observability Configuration

Add to `config.toml`:

```toml
[observability]
enabled = true
database_path = "./data/observability.db"

# Performance tuning
[observability.performance]
batch_size = 100              # Events per batch write
flush_interval_ms = 100       # Max time before flushing batch
max_buffer_size = 10000       # Ring buffer size

# Data retention policies
[observability.retention]
logs_days = 7                     # Keep request logs for 7 days
spans_days = 7                    # Keep trace spans for 7 days
cleanup_hour = 3                  # Run cleanup at 3 AM daily (0-23)

# Quota refresh configuration (Anthropic OAuth only)
[observability.quota_refresh]
enabled = true              # Enable quota monitoring
interval_seconds = 600      # Refresh interval (default: 10 minutes)
timeout_seconds = 30        # Query timeout (default: 30 seconds)
retention_days = 7          # Data retention (default: 7 days)
```

### Environment Variables

You can override configuration with environment variables:

```bash
export LLM_GATEWAY__SERVER__PORT=9000
export LLM_GATEWAY__PROVIDERS__OPENAI__API_KEY="sk-new-key"
export LLM_GATEWAY__OBSERVABILITY__ENABLED=true
```

### OAuth Authentication

The gateway supports OAuth 2.0 authentication for provider instances, particularly useful for Anthropic Claude.

**Why Use OAuth?**
- Enables quota monitoring for Anthropic instances
- Automatic token refresh in the background
- More secure than storing API keys directly
- Required for accessing Anthropic's usage API

**Configuration**:

```toml
# OAuth provider configuration
[[oauth_providers]]
name = "anthropic"
client_id = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
auth_url = "https://claude.ai/oauth/authorize"
token_url = "https://console.anthropic.com/v1/oauth/token"
redirect_uri = "https://platform.claude.com/oauth/code/callback"
scopes = ["org:create_api_key", "user:profile", "user:inference", "user:sessions:claude_code"]

# Provider instance using OAuth
[[providers.anthropic]]
name = "anthropic-oauth"
enabled = true
auth_mode = "oauth"
oauth_provider = "anthropic"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"

# Provider instance using API Key (no quota monitoring)
[[providers.anthropic]]
name = "anthropic-key"
enabled = true
auth_mode = "bearer"
api_key = "sk-ant-your-key"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"
```

**OAuth Management**:

```bash
# Login (initiates OAuth flow in browser)
./target/release/llm-gateway oauth login anthropic

# Check OAuth status
./target/release/llm-gateway oauth status anthropic

# Refresh token manually
./target/release/llm-gateway oauth refresh anthropic

# Logout
./target/release/llm-gateway oauth logout anthropic
```

**Token Storage**:
- Encrypted storage in `~/.llm-gateway/oauth_tokens.json`
- Automatic refresh 5 minutes before expiration
- Background refresh task runs every 5 minutes

### Authentication Modes Comparison

| Feature | Bearer (API Key) | OAuth |
|---------|:----------------:|:-----:|
| **Configuration** | Simple API key | OAuth flow required |
| **Token Storage** | Plaintext in config | Encrypted in ~/.llm-gateway/ |
| **Token Refresh** | Manual | Automatic (5 min before expiry) |
| **Quota Monitoring** | ❌ Not available | ✅ Available (Anthropic only) |
| **Setup Complexity** | Low | Medium |
| **Use Case** | Testing, quick start | Production, monitoring |
| **Security** | API key in file | Encrypted, auto-refreshed |
| **Anthropic Usage API** | ❌ No access | ✅ Full access |

**Recommendation**:
- **Development/Testing**: Use Bearer mode for simplicity
- **Production**: Use OAuth mode for automatic quota monitoring and better security
- **Hybrid**: Run both OAuth and Bearer instances for redundancy

**Example Hybrid Configuration**:

```toml
# Primary instance: OAuth (enables quota monitoring)
[[providers.anthropic]]
name = "anthropic-primary"
enabled = true
auth_mode = "oauth"
oauth_provider = "anthropic"
priority = 1

# Backup instance: Bearer (fallback)
[[providers.anthropic]]
name = "anthropic-backup"
enabled = true
auth_mode = "bearer"
api_key = "sk-ant-backup-key"
priority = 2
```

In this setup:
- Normal traffic uses OAuth instance (with quota monitoring)
- If OAuth fails, automatically fails over to Bearer instance
- `llm-gateway stats` shows quota info for primary, N/A for backup

```bash
export LLM_GATEWAY__SERVER__PORT=9000
export LLM_GATEWAY__PROVIDERS__OPENAI__API_KEY="sk-new-key"
export LLM_GATEWAY__OBSERVABILITY__ENABLED=true
```

## License

MIT

## FAQ

### Quota Monitoring

**Q: Why is quota monitoring only available for Anthropic OAuth?**

A: Anthropic provides a dedicated usage API endpoint (`https://api.anthropic.com/api/oauth/usage`) that requires OAuth authentication. The API Key (bearer) authentication mode does not have access to quota query APIs. This is a limitation of Anthropic's API design, not the gateway.

**Q: Can I use both OAuth and Bearer instances together?**

A: Yes! You can configure multiple Anthropic instances with different authentication modes. The gateway will automatically route traffic between them based on priority and health. Only OAuth instances will show quota information in the stats output.

**Q: How often is quota data refreshed?**

A: By default, quota data is refreshed every 10 minutes. You can adjust this in the configuration:

```toml
[observability.quota_refresh]
interval_seconds = 600  # 10 minutes (default)
```

**Q: What happens if quota query fails?**

A: The gateway gracefully handles quota query failures:
- Failed queries are logged but don't affect traffic routing
- Status shows "✗ ERROR" in the stats output
- Next refresh cycle will attempt to query again
- Provider instances continue to serve requests normally

**Q: Can I monitor quotas for OpenAI or Gemini?**

A: Currently, only Anthropic OAuth supports quota monitoring. OpenAI and Gemini do not provide public APIs for quota queries. If these providers add such APIs in the future, the gateway's modular architecture makes it easy to add support.

**Q: How accurate is the quota data?**

A: The quota data comes directly from Anthropic's usage API and reflects real-time usage across all applications using your OAuth credentials. The data includes:
- Utilization percentage (0-100%)
- Reset timestamps for each usage window
- Separate tracking for general usage and Sonnet-specific usage

**Q: Will bearer mode instances ever support quota monitoring?**

A: Only if Anthropic releases a quota query API for API Key authentication. The gateway is designed to be extensible, so if such an API becomes available, we can add support for it.

**Q: How can I set up alerts for quota limits?**

A: While the gateway doesn't have built-in alerting, you can:
1. Run `llm-gateway stats` periodically in a cron job
2. Query the SQLite database directly
3. Use the Dashboard API endpoints (`/api/instances/current-health`)
4. Parse the output and send alerts when utilization exceeds a threshold

Example cron job:
```bash
# Check quota every hour and alert if >80%
0 * * * * /path/to/llm-gateway stats | grep -q "5h: 8[0-9]\." && echo "High quota usage!" | mail -s "Alert" admin@example.com
```

## Architecture Details

See the implementation plan in the repo for full architecture documentation including:
- Three-endpoint design
- Model routing logic
- Protocol conversion strategies
- Streaming architecture
- Metrics implementation

Built with ❤️ in Rust using Axum, Tokio, and SQLite.
