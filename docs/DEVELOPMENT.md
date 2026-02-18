# Development Guide

This document contains technical architecture details, development workflows, and internal implementation documentation for LLM Gateway contributors.

For user-facing documentation (installation, configuration, usage), see the [README](../README.md).

## Provider Architecture (Trait-based)

The gateway uses a trait-based pluggable provider system:

```
ProviderConfig trait          LlmProvider trait           ProviderRegistry
(instance configuration)      (request sending)           (string-keyed lookup)
┌──────────────────┐         ┌──────────────────┐        ┌──────────────────┐
│ name()           │         │ provider_type()  │        │ "openai"         │
│ enabled()        │         │ native_protocol()│        │ "anthropic"      │
│ auth_mode()      │         │ send_request()   │        │ "azure_openai"   │
│ api_key()        │         │ health_check_url│        │ "bedrock"        │
│ base_url()       │         └──────────────────┘        │ "custom:deepseek"│
│ as_any() → downcast│                                    └──────────────────┘
└──────────────────┘
```

Adding a new provider requires:
1. Config struct implementing `ProviderConfig`
2. Provider struct implementing `LlmProvider`
3. Registration in `create_provider_registry()`
4. (Optional) Path-routed handler + route

If prefix routing is needed (via `/v1/chat/completions`):
5. Add prefix → provider mapping in `routing.rules`
6. If not OpenAI-compatible, add converter in `src/converters/`

## Load Balancing & High Availability

### Multi-Provider Instance Architecture

Each provider type can have **multiple backend instances** for load balancing and automatic failover:

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

#### Circuit Breaker Pattern

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
- Fully stateless gateways (no cross-process communication)
- No Redis/shared state required
- Extreme performance (two memory-only hash lookups)
- Easy scaling (just add/remove gateway instances in Nginx upstream)
- Fault isolation (one gateway failure doesn't affect others)

## Observability Internals

### Failover Event Tracking

The gateway tracks all circuit breaker events in the `failover_events` table:

**Event Types:**
- `failure` - Instance failure recorded
- `circuit_open` - Circuit breaker opened (3 failures)
- `circuit_half_open` - Testing recovery (health check passed)
- `circuit_closed` - Circuit closed (2 successes)
- `recovery` - Instance recovered

### SQL Query Examples

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

## Protocol Conversion

The gateway automatically converts between protocols:

| Feature | OpenAI | Anthropic | Gemini | Conversion |
|---------|--------|-----------|--------|------------|
| System message | `messages[0].role="system"` | `system` field | `systemInstruction` | Extracted |
| Role names | `assistant` | `assistant` | `model` | Mapped |
| max_tokens | Optional | Required | Optional | Default: 4096 |
| temperature | 0-2 | 0-1 | 0-2 | Clipped |
| Content blocks | String or array | String or array | Parts array | Converted |
| Tools | OpenAI format | Anthropic format | Function declarations | Converted |
| Images | URL or base64 | Base64 only | Base64 only | Auto-converted |

## Building & Testing

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

**Important:** When using `cross` directly, always run it from the **project root directory** (not the `backend` directory), and use `--manifest-path backend/Cargo.toml`.

**Binary sizes:**
- macOS (release): ~10MB
- Linux (release): ~10MB
- Linux MUSL (static): ~12MB

**Troubleshooting:**

If you encounter OpenSSL-related errors, ensure you're using the latest code which has switched from `native-tls` to `rustls` (pure Rust SSL implementation).

## Advanced Configuration

### Observability Configuration

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

## Related Documentation

- **[IMPLEMENTATION.md](IMPLEMENTATION.md)** - Complete implementation details and architecture
- **[FEATURES.md](FEATURES.md)** - Comprehensive feature documentation
- **[CONVERSION_LIMITATIONS.md](CONVERSION_LIMITATIONS.md)** - Provider conversion trade-offs
- **[DAEMON.md](DAEMON.md)** - Running as a daemon/background service
