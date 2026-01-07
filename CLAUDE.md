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

## Development Best Practices

### Data Model Design

#### 1. Avoid Over-Strict Type Definitions for External Data

**Problem**: Using strict Rust types with required fields for data from external clients can cause deserialization failures.

**Example of problematic code**:
```rust
// ❌ BAD: Too strict for external data
pub struct ThinkingBlock {
    pub thinking: Option<String>,
    pub signature: String,  // Required field!
}

#[serde(untagged)]
pub enum ThinkingContent {
    String(String),
    Block(ThinkingBlock),  // Will fail if signature is missing
}
```

**Problem**: If official clients (like Claude Code CLI) send slightly different formats, deserialization fails entirely.

**Solution**: Use `serde_json::Value` for fields that don't need validation at gateway level:
```rust
// ✅ GOOD: Flexible for external data
pub struct ContentBlock {
    // ... other fields ...

    /// Accepts any format, forwarded as-is to upstream API
    /// Validation is done by upstream API, not gateway
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
}
```

**When to use `Value` vs strong types**:
- Use `Value` for:
  - Fields only passed through (not processed by gateway)
  - Fields where upstream API does validation
  - Fields with multiple possible formats
  - Data from official/external clients

- Use strong types for:
  - Fields you need to read/modify in gateway
  - Fields where you do business logic
  - Internal data structures
  - Configuration files you control

#### 2. Beware of `#[serde(untagged)]` Enum Pitfalls

**Problem**: With `#[serde(untagged)]`, serde tries all variants sequentially. If ANY field fails in ALL variants, the entire deserialization fails.

```rust
// ❌ DANGEROUS: One bad field kills everything
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),  // If ContentBlock has strict types, this fails
}
```

**Impact**: A single invalid field in one content block causes the entire message to fail.

**Mitigation**:
- Make inner types flexible (use `Value` for pass-through fields)
- Add custom deserialize functions with fallback behavior
- Log warnings instead of errors for optional fields

#### 3. Gateway Responsibility: Forward, Not Validate

**Core Principle**: The gateway's job is routing and forwarding, NOT validating upstream API contracts.

```rust
// ❌ BAD: Gateway enforcing upstream API rules
fn sanitize_thinking_fields(request: &mut MessagesRequest) {
    // Removing fields the client sent!
    if !is_valid_thinking_format(&block.thinking) {
        block.thinking = None;  // Data loss!
    }
}

// ✅ GOOD: Forward as-is, let upstream validate
let request: MessagesRequest = serde_json::from_value(raw_request)?;
// No modification - send exactly what client provided
providers::anthropic::create_message(&client, config, request).await
```

**Why**:
- Official clients (Claude Code CLI) send correct formats
- If gateway removes fields, information is lost
- Upstream API will return proper error if format is wrong
- Gateway shouldn't second-guess official clients

#### 4. Official Client Compatibility is Critical

**Always remember**: Clients like Claude Code CLI are official tools from the same company (Anthropic). If the gateway can't handle their requests, **the gateway is wrong, not the client**.

**Checklist when adding new models**:
- [ ] Can the model accept all variants official clients might send?
- [ ] Are required fields actually required by the spec, or just convenient?
- [ ] Will strict validation break compatibility with future client versions?
- [ ] Is there a reference implementation (like claude-relay-service) to compare against?

### Testing Strategy

#### Test with Real Client Payloads

Don't just test with synthetic data. Test with actual payloads from:
- Claude Code CLI
- Official SDK examples
- Production traffic (sanitized)

```rust
#[test]
fn test_real_claude_code_cli_request() {
    // Actual payload from Claude Code CLI (sanitized)
    let json = r#"{
        "model": "claude-3-5-sonnet-20241022",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "text",
                "text": "Hello",
                "thinking": {"thinking": "...", "signature": "..."}
            }]
        }],
        "max_tokens": 1024
    }"#;

    let request: MessagesRequest = serde_json::from_str(json).unwrap();
    assert!(request.messages[0].content.is_some());
}
```

#### Test All Format Variants

For fields that accept multiple formats, test ALL of them:

```rust
#[test]
fn test_thinking_field_formats() {
    // String format
    test_deserialize(r#"{"thinking": "text"}"#);

    // Object without optional fields
    test_deserialize(r#"{"thinking": {"thinking": "text"}}"#);

    // Object with all fields
    test_deserialize(r#"{"thinking": {"thinking": "text", "signature": "sig"}}"#);

    // Null/missing
    test_deserialize(r#"{}"#);
}
```

### Debugging Deserialization Failures

When you see errors like "data did not match any variant of untagged enum":

1. **Log the raw JSON** before deserialization:
```rust
Json(raw_request): Json<serde_json::Value>
) -> Result<Response, AppError> {
    tracing::debug!(request = ?raw_request, "Received raw request");

    let request: MessagesRequest = serde_json::from_value(raw_request.clone())
        .map_err(|e| {
            tracing::error!(
                error = %e,
                sample = ?serde_json::to_string(&raw_request).ok(),
                "Deserialization failed"
            );
            // ...
        })?;
}
```

2. **Check reference implementations** (like claude-relay-service in Node.js):
   - How do they handle the same field?
   - Do they use strict types or flexible objects?
   - What formats do they accept?

3. **Identify the strict type** causing the failure:
   - Look for required fields in structs
   - Look for `#[serde(untagged)]` enums with strict variants
   - Look for custom deserialize functions that might fail

4. **Relax the type** instead of filtering data:
   - Change to `Option<serde_json::Value>`
   - Add `#[serde(default)]` for non-critical fields
   - Use custom deserialize with fallback

### Error Handling Patterns

#### Distinguish Gateway Errors from Upstream Errors

```rust
// In src/retry.rs
pub fn is_instance_failure(error: &AppError) -> bool {
    match error {
        // Gateway/network issues - trigger failover
        AppError::HttpClientError(_) => true,
        AppError::UpstreamError { status, .. } if status.is_server_error() => true,

        // Business/validation errors - DON'T trigger failover
        AppError::ConversionError(_) => false,  // Client sent bad data
        AppError::UpstreamError { status, .. } if status.is_client_error() => false,

        _ => false,
    }
}
```

**Rationale**: Deserialization failures are usually client errors or gateway bugs, NOT provider failures. Don't mark providers unhealthy for these.

### Common Mistakes to Avoid

1. ❌ **Adding validation that upstream API already does**
   - If Anthropic API validates `thinking.signature`, don't duplicate this in gateway

2. ❌ **Removing fields you don't understand**
   - Unknown fields should be preserved and forwarded

3. ❌ **Making fields required "for convenience"**
   - Only make fields required if the upstream API spec requires them

4. ❌ **Not testing with official clients**
   - Always test with Claude Code CLI, official SDKs, etc.

5. ❌ **Assuming your type definition is "correct"**
   - The official client's format is the source of truth, not your Rust struct

### Version Compatibility

When upstream APIs add new fields or formats:

- ✅ **Gateway should work without code changes** (if using `Value` for pass-through fields)
- ✅ **Clients can adopt new features immediately** (gateway doesn't block)
- ❌ **Don't require gateway updates** for every upstream API change

This is why `serde_json::Value` is preferred for fields that are only passed through.
