# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LLM Gateway is a high-performance Rust proxy that provides a unified OpenAI-compatible API for multiple LLM providers (OpenAI, Anthropic, Google Gemini). It features multi-instance load balancing with sticky sessions, automatic failover, and zero external dependencies (no Redis/database).

**Version**: 0.3.0
**Stack**: Rust + Axum + Tokio + Prometheus

## Essential Commands

### Building & Running
```bash
# Build debug
cargo build

# Build release (required for optimal performance)
cargo build --release

# Run debug
cargo run

# Run release
cargo run --release

# Test configuration without starting
cargo run --release -- test
# or: ./target/release/llm-gateway test
```

### Testing
```bash
# Run all tests
cargo test

# Run only unit tests (lib tests)
cargo test --lib

# Run specific test
cargo test test_name

# Run with output visible
cargo test -- --nocaptures
```

### Configuration Management
```bash
# Validate config
./target/release/llm-gateway config validate

# Show config (secrets masked)
./target/release/llm-gateway config show
```

### Running the Server
```bash
# Start in foreground
./target/release/llm-gateway start

# With custom config path
./target/release/llm-gateway --config /path/to/config.toml start
```

### Important Files
- `config.toml` - **Never commit with real API keys** (in .gitignore)
- `config.toml.example` - Template for configuration

## Architecture Overview

### Request Flow (Multi-Layer)

```
Client Request
    ↓
Auth Middleware (src/auth.rs) - validates API key
    ↓
ModelRouter (src/router.rs) - prefix-based routing (e.g., "gpt-" → openai)
    ↓
LoadBalancer (src/load_balancer.rs) - sticky session selection
    ↓
Retry Layer (src/retry.rs) - health detection & metrics
    ↓
Protocol Converter (src/converters/*) - if needed
    ↓
Provider (src/providers/*) - actual LLM API call
```

### Core Components

#### 1. Load Balancing System (NEW in v0.3.0)

**Files**: `src/load_balancer.rs`, `src/retry.rs`

The load balancer implements **priority-based sticky sessions**:

- **Sticky Sessions**: Each API key binds to a specific provider instance for 1 hour
  - Maximizes KV cache hits at provider side
  - Uses DashMap (segment locking) for low contention

- **Priority Selection**: Lower priority number = higher priority
  - Same priority instances = random selection among them
  - Only selects from healthy, enabled instances

- **Health Management**:
  - Single failure (5xx, timeout, connection error) → marks instance unhealthy
  - 4xx errors (auth, rate limit) → **do not** mark unhealthy
  - Auto-recovery after `failure_timeout_seconds` (default: 60s)
  - Background tasks: health recovery loop (10s), session cleanup (5min)

- **Gradual Recovery**: After instance recovers, users stay on backup until session expires (anti-flapping)

**Key Methods**:
- `LoadBalancer::select_instance_for_key()` - returns instance for API key (sticky)
- `LoadBalancer::mark_instance_failure()` - marks instance unhealthy
- `is_instance_failure()` in retry.rs - determines if error is instance-level

#### 2. Model Routing

**File**: `src/router.rs`

Uses **prefix matching** from `config.toml`:
```toml
[routing.rules]
"gpt-" = "openai"
"claude-" = "anthropic"
"gemini-" = "gemini"
```

Validates model names (alphanumeric + `-._/` only, 1-256 chars) to prevent injection.

#### 3. Protocol Conversion

**Files**: `src/converters/openai_to_anthropic.rs`, `openai_to_gemini.rs`, `anthropic_to_openai.rs`

Bidirectional conversion between OpenAI ↔ Anthropic ↔ Gemini formats:

**Key Differences**:
- **System messages**: OpenAI uses `messages[0]`, Anthropic uses `system` field, Gemini uses `systemInstruction`
- **Role names**: OpenAI/Anthropic use `assistant`, Gemini uses `model`
- **max_tokens**: Required for Anthropic, optional for others (defaults to 4096)
- **temperature**: Anthropic is 0-1, others 0-2 (clips automatically)

**Critical**: Converters handle streaming responses too (see `src/streaming.rs`)

#### 4. Configuration System

**File**: `src/config.rs`

Multi-instance provider configuration (v0.3.0):
```toml
[[providers.anthropic]]
name = "anthropic-primary"
enabled = true
api_key = "sk-ant-..."
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
priority = 1                      # Lower = higher priority
failure_timeout_seconds = 60      # Auto-recovery timeout
```

**Important**: Providers are now **arrays** (`Vec<ProviderInstanceConfig>`), not single instances.

#### 5. Metrics System

**File**: `src/metrics.rs`

Prometheus metrics with four dimensions: `api_key`, `provider`, `model`, `instance`

**Key Metrics**:
- `llm_requests_total` - request count
- `llm_tokens_total` - token usage (input/output)
- `llm_request_duration_seconds` - latency histogram
- `llm_instance_health_status` - instance health (1=healthy, 0=unhealthy)
- `llm_instance_requests_total` - per-instance request count with status
- `llm_gateway_session_count` - active sticky sessions

Metrics are recorded in `retry.rs` during request execution.

### Handlers

**Directory**: `src/handlers/`

- `chat_completions.rs` - `/v1/chat/completions` (OpenAI-compatible)
- `messages.rs` - `/v1/messages` (native Anthropic API)
- `models.rs` - `/v1/models` (model listing)
- `health.rs` - `/health`, `/ready` endpoints

**Important Pattern**: All handlers use `execute_with_session()` from `retry.rs` to integrate with load balancing and metrics.

## Configuration Patterns

### Multi-Instance Setup

Each provider type can have multiple instances with different priorities:

```toml
# Primary instance (always preferred)
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1
# ... config ...

# Backup instance (only used if primary fails)
[[providers.anthropic]]
name = "anthropic-backup"
priority = 2
# ... config ...

# Same-priority backup (random selection between this and backup)
[[providers.anthropic]]
name = "anthropic-backup-2"
priority = 2
# ... config ...
```

### Routing Configuration

```toml
[routing]
default_provider = "openai"  # Fallback if no prefix matches

[routing.rules]
"gpt-" = "openai"
"claude-" = "anthropic"

[routing.discovery]
enabled = true
cache_ttl_seconds = 3600
providers_with_listing = ["openai"]  # Providers supporting model listing API
```

## Testing Patterns

### Test Configuration Helpers

When writing tests that need Config, use the pattern from existing tests:

```rust
use crate::config::{
    ProviderInstanceConfig, AnthropicInstanceConfig,
    ProvidersConfig, RoutingConfig, // ... other config types
};

fn create_test_config() -> Config {
    Config {
        // ...
        providers: ProvidersConfig {
            openai: vec![ProviderInstanceConfig { /* ... */ }],
            anthropic: vec![AnthropicInstanceConfig { /* ... */ }],
            gemini: vec![ProviderInstanceConfig { /* ... */ }],
        },
        // ...
    }
}
```

**Important**: Providers are arrays, not single objects (changed in v0.3.0).

### Middleware Tests

Auth middleware tests in `src/auth.rs` are marked `#[ignore]` due to trait bound issues in test environment. These are covered by integration tests instead.

## Common Modification Patterns

### Adding a New Provider

1. Add enum variant to `Provider` in `src/router.rs`
2. Add config struct to `src/config.rs` (in `ProvidersConfig`)
3. Create provider module in `src/providers/`
4. Add converters in `src/converters/` if not OpenAI-compatible
5. Update `build_load_balancers()` in `src/server.rs`
6. Update routing rules in config

### Adding Instance-Level Metrics

Metrics are recorded in `src/retry.rs::execute_with_session()`:
- Success: `record_instance_request(provider, instance, "success")`
- Instance failure: `record_instance_request(provider, instance, "failure")`
- Business error: `record_instance_request(provider, instance, "business_error")`

Health metrics are auto-updated in `LoadBalancer` methods.

### Modifying Health Detection

Edit `is_instance_failure()` in `src/retry.rs` to change which errors trigger failover:
- Return `true` = mark instance unhealthy, trigger failover
- Return `false` = treat as business error, no failover

Current triggers: 5xx status, connection errors, timeouts

## Security Considerations

- **API Keys**: Never commit `config.toml` with real keys (use `config.toml.example`)
- **Model Name Validation**: Router validates model names to prevent injection (alphanumeric + `-._/` only)
- **Request Size Limit**: 10MB max body size (configured in `src/server.rs`)
- **Authentication**: Bearer token auth via middleware (`src/auth.rs`)

## Performance Notes

- **Lock Strategy**: DashMap (segment locking) for sessions + RwLock (read-heavy) for health state
- **Zero Allocations**: Uses Arc for shared config/instances
- **Session TTL**: 1 hour inactivity timeout, auto-refresh on request
- **Background Tasks**: Session cleanup (5min), health recovery (10s)

## Deployment

### Docker
```bash
docker build -t llm-gateway .
docker run -p 8080:8080 -v $(pwd)/config.toml:/app/config.toml llm-gateway
```

### Horizontal Scaling with Nginx
Use consistent hashing on `Authorization` header for two-layer stickiness:
```nginx
upstream llm_gateway_cluster {
    hash $http_authorization consistent;
    server gateway-1:8080;
    server gateway-2:8080;
}
```

This ensures:
- Layer 1 (Nginx): API key → specific gateway instance
- Layer 2 (Gateway): API key → specific provider instance
- Result: Maximum KV cache hits, no shared state needed
