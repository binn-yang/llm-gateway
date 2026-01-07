# LLM Gateway

A high-performance, minimal LLM proxy gateway written in Rust that provides a unified OpenAI-compatible API for multiple LLM providers (OpenAI, Anthropic Claude, Google Gemini).

## Features

- **Unified OpenAI API**: Single `/v1/chat/completions` endpoint supports all providers
- **Protocol Conversion**: Automatic request/response translation between OpenAI, Anthropic, and Gemini formats
- **Smart Routing**: Prefix-based model routing to appropriate providers
- **Multi-Instance Load Balancing**: Each provider supports multiple backend instances with priority-based selection
- **Sticky Sessions**: API key-level session affinity maximizes provider-side KV cache hits
- **Automatic Failover**: Single request failure triggers instant failover with auto-recovery
- **Zero Dependencies**: No database, Redis, or cache required - just binary + config file
- **Static Authentication**: API key-based auth configured in TOML
- **Prometheus Metrics**: Four-dimension metrics with instance-level observability
- **Streaming Support**: Full SSE support with real-time protocol conversion
- **Cloud Native**: Docker ready, health checks, structured JSON logging
- **Horizontal Scaling**: Nginx-compatible for multi-machine deployments

## Architecture

```
┌─────────────┐
│   Cursor    │  (OpenAI endpoint)
│ Claude Code │  → /v1/chat/completions → Gateway → Auto-routes to:
│   etc.      │                                    ├─ OpenAI (direct)
└─────────────┘                                    ├─ Anthropic (converted)
                                                   └─ Gemini (converted)
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

An instance is marked **unhealthy** on **single request failure** of these types:

| Failure Type | Examples | Action |
|--------------|----------|--------|
| **5xx Server Errors** | 500, 502, 503, 504 | Mark unhealthy |
| **Connection Failures** | TCP timeout, connection refused, DNS failure | Mark unhealthy |
| **Request Timeouts** | Exceeds `timeout_seconds` | Mark unhealthy |
| **4xx Client Errors** | 401, 403, 429 | **No action** (not instance fault) |
| **Business Errors** | Invalid API key, rate limit | **No action** |

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
cargo run --release
```

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/health` | GET | No | Health check |
| `/ready` | GET | No | Readiness check |
| `/metrics` | GET | No | Prometheus metrics |
| `/v1/chat/completions` | POST | Yes | OpenAI-compatible chat completion (supports all models) |
| `/v1/models` | GET | Yes | List available models |

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
# If Claude Code supports OpenAI format:
export ANTHROPIC_BASE_URL="http://localhost:8080/v1"
export ANTHROPIC_API_KEY="sk-gateway-001"
```

### Direct API Call

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

## Monitoring

### Prometheus Metrics

Access metrics at `/metrics`:

```promql
# Request count
llm_requests_total{api_key="my-app", provider="anthropic", model="claude-3-5-sonnet"}

# Token usage
llm_tokens_total{api_key="my-app", provider="anthropic", model="claude-3-5-sonnet", type="input"}

# Request duration
llm_request_duration_seconds{api_key="my-app", provider="anthropic", model="claude-3-5-sonnet"}

# Error count
llm_errors_total{api_key="my-app", provider="anthropic", error_type="rate_limit"}
```

## Protocol Conversion

The gateway automatically converts between protocols:

| Feature | OpenAI | Anthropic | Gemini | Conversion |
|---------|--------|-----------|--------|------------|
| System message | `messages[0].role="system"` | `system` field | `systemInstruction` | ✅ Extracted |
| Role names | `assistant` | `assistant` | `model` | ✅ Mapped |
| max_tokens | Optional | Required | Optional | ✅ Default: 4096 |
| temperature | 0-2 | 0-1 | 0-2 | ✅ Clipped |

## Development

### Running Tests

```bash
cargo test
```

### Building Release Binary

```bash
cargo build --release
```

## Configuration

### Environment Variables

You can override configuration with environment variables:

```bash
export LLM_GATEWAY__SERVER__PORT=9000
export LLM_GATEWAY__PROVIDERS__OPENAI__API_KEY="sk-new-key"
```

## License

MIT

## Architecture Details

See the implementation plan in the repo for full architecture documentation including:
- Three-endpoint design
- Model routing logic
- Protocol conversion strategies
- Streaming architecture
- Metrics implementation

Built with ❤️ in Rust using Axum, Tokio, and Prometheus.
