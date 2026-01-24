# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LLM Gateway is a high-performance Rust proxy that provides multiple API formats for LLM providers (OpenAI, Anthropic, Google Gemini):
- **Unified OpenAI-compatible API** (`/v1/chat/completions`) - works with all providers via automatic protocol conversion
- **Native Anthropic Messages API** (`/v1/messages`) - direct passthrough for Claude models without conversion overhead

It features multi-instance load balancing with sticky sessions, automatic failover, SQLite-based observability system with web dashboard, and complete token tracking including Anthropic prompt caching metrics.

**Version**: 0.4.0
**Stack**: Backend (Rust + Axum + Tokio + SQLite) + Frontend (Vue 3 + TypeScript + Chart.js)

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

The gateway supports two API formats with different processing flows:

**Flow 1: OpenAI-compatible API** (`/v1/chat/completions`)
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
Protocol Converter (src/converters/*) - if needed (Anthropic/Gemini)
    ↓
Provider (src/providers/*) - actual LLM API call
```

**Flow 2: Native Anthropic API** (`/v1/messages`)
```
Client Request (native Anthropic format)
    ↓
Auth Middleware (src/auth.rs) - validates API key
    ↓
LoadBalancer (src/load_balancer.rs) - sticky session selection
    ↓
Retry Layer (src/retry.rs) - health detection & metrics
    ↓
Provider (src/providers/anthropic.rs) - direct Anthropic API call (no conversion)
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

#### 5. Observability System (NEW in v0.4.0)

**Files**: `src/observability/request_logger.rs`, `backend/migrations/*.sql`

SQLite-based observability system with async non-blocking writes:

**Architecture**:
- **RequestLogger**: Async logger with ring buffer (10,000 events)
- **Batch Writing**: 100 events per batch, 100ms flush interval
- **Non-blocking**: Uses tokio channels, never blocks request handling
- **Data Retention**: Automatic cleanup (logs 7 days, traces 7 days, metrics 30 days)

**Database Tables**:
- **requests**: Per-request logs with token usage and performance metrics
- **spans**: Distributed tracing spans (not yet fully implemented)

**Key Fields in requests table**:
- Basic: `request_id`, `timestamp`, `api_key_name`, `provider`, `instance`, `model`, `endpoint`
- Token Usage: `input_tokens`, `output_tokens`, `total_tokens`
- **Caching**: `cache_creation_input_tokens`, `cache_read_input_tokens` (Anthropic prompt caching)
- Performance: `duration_ms`, `status`, `error_type`, `error_message`

**Usage Pattern**:
```rust
// In handlers
let logger = request_logger.clone();
logger.log_request(RequestEvent {
    request_id: request_id.clone(),
    timestamp: Utc::now().timestamp_millis(),
    // ... other fields ...
    input_tokens: tokens.0 as i64,
    output_tokens: tokens.1 as i64,
    cache_creation_input_tokens: tokens.2 as i64,
    cache_read_input_tokens: tokens.3 as i64,
    // ...
}).await;
```

#### 6. File-Based Logging System (NEW in v0.4.0)

**Files**: `src/lib.rs` (log configuration), `src/handlers/dashboard_api.rs` (query API), `src/server.rs` (cleanup)

Simple JSONL-based logging system for detailed request tracing:

**Architecture**:
- **Dual Output**: Console (human-readable) + Files (JSONL format)
- **Async Writes**: Uses `tracing-appender::non_blocking` for zero-latency logging
- **Daily Rotation**: Automatic file rotation by date (`requests.YYYY-MM-DD`)
- **Auto Cleanup**: Deletes files older than 7 days on startup

**File Format** (`logs/requests.YYYY-MM-DD`):
```json
{
  "timestamp": "2026-01-21T12:26:52.912136Z",
  "level": "INFO",
  "fields": {"message": "Request started", "stream": false},
  "target": "llm_gateway::handlers::messages",
  "span": {
    "request_id": "e23a4a4f-81bc-4366-ae50-b852d7493630",
    "api_key_name": "y111",
    "model": "claude-3-5-sonnet-20241022",
    "endpoint": "/v1/messages",
    "provider": "anthropic",
    "instance": "anthropic-primary"
  }
}
```

**Key Features**:
- **Request ID as Trace ID**: Every request gets a UUID that appears in logs, response headers (`X-Request-ID`), and database records
- **Span Context**: Using `tracing::Span`, all logs within a request automatically include `request_id`, `api_key_name`, `model`, `provider`, `instance`
- **Structured Logging**: JSONL format allows grep, jq, and other tools for analysis

**Query API**:
```bash
# Get last 3 logs (default)
GET /api/dashboard/logs

# Get last 100 logs
GET /api/dashboard/logs?limit=100

# Trace query (all logs for a specific request)
GET /api/dashboard/logs?request_id=e23a4a4f-81bc-4366-ae50-b852d7493630

# Text search
GET /api/dashboard/logs?grep=error

# Specific date
GET /api/dashboard/logs?date=2026-01-21
```

**Response**:
```json
{
  "logs": [/* array of JSON log objects */],
  "total": 3,
  "files_searched": ["requests.2026-01-21"]
}
```

**Implementation Notes**:
- Logs are independent of SQLite database (requests table stores aggregated metrics, logs/ stores detailed traces)
- `Box::leak` is used for `_guard` to keep log writer alive for application lifetime
- Query API reads files directly (no database dependency)
- Searches last 2 days by default (today + yesterday)

#### 7. Token Tracking (Enhanced in v0.4.0)

**Files**: `src/streaming.rs`, `src/handlers/*`

Complete token tracking including Anthropic prompt caching metrics:

**StreamingUsageTracker**:
- Tracks 4 token types: input, output, cache_creation, cache_read
- **Unified Extraction**: All tokens extracted from `message_delta` event for compatibility
- **Provider Compatibility**: Works with Anthropic official API and GLM provider
- **Non-blocking**: Uses tokio watch channel for completion notification

**Key Methods**:
- `set_full_usage()`: Set all 4 token types from message_delta (preferred method)
- `set_input_usage()`: Legacy method for partial updates
- `wait_for_completion()`: Async wait for token data (no polling)

**Extraction Strategy** (Optimized for GLM compatibility):
```rust
// OpenAI-compatible API stream (src/streaming.rs ~420)
"message_delta" => {
    if let Some(usage) = &anthropic_event.usage {
        tracker.set_full_usage(
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_creation_input_tokens,
            usage.cache_read_input_tokens,
        );
    }
}
```

**Why from message_delta only**:
- Anthropic official API: Complete data in both message_start and message_delta (redundant)
- GLM provider: message_start returns zeros, message_delta has complete data
- Solution: Unified extraction from message_delta works for all implementations

**Caching Cost Analysis**:
- `cache_creation_input_tokens`: Tokens used to create cache (+25% cost)
- `cache_read_input_tokens`: Tokens read from cache (-90% cost)
- Regular tokens: `input_tokens - cache_creation - cache_read`

#### 8. Metrics System

**Files**: `src/observability/*` (SQLite-based)

**SQLite-Based Metrics**:

All metrics are stored in SQLite for unified observability:
- **Per-request granularity**: Every request logged with full token usage details
- **Cache metrics**: Tracks cache_creation_input_tokens and cache_read_input_tokens
- **Instance health**: Recorded per request with provider/instance labels
- **Time-series data**: Queryable via SQL for custom analytics
- **Automatic retention**: 30-day retention policy with daily cleanup
- **Dashboard integration**: Powers Vue 3 frontend charts via REST API

**Key Metrics Available**:
- Request count and status by provider/model/api_key
- Token usage (input/output/cache) with cost analysis
- Request duration and latency percentiles
- Instance health status and failover events
- Active session tracking (via load balancer state)

Metrics are recorded in `src/observability/request_logger.rs` with async batch writes (100 events per batch, 100ms flush interval).

#### 9. Configuration Management System (NEW in v0.5.0)

**Files**: `src/config_db.rs`, `src/handlers/config_api.rs`, `backend/migrations/20260122000001_add_config_tables.sql`

Database-driven configuration system with hot reload capability:

**Architecture**:
- **Database-First**: Configuration stored in SQLite (`./data/config.db`)
- **Hot Reload**: Changes take effect immediately without server restart
- **TOML Fallback**: Initial setup from `config.toml`, then database takes over
- **Web UI**: Vue 3 frontend for CRUD operations on API keys, routing rules, and provider instances

**Database Tables**:
- **api_keys**: Gateway authentication keys (SHA256 hashed)
- **routing_rules**: Model prefix to provider mappings
- **provider_instances**: Backend provider configurations (OpenAI, Anthropic, Gemini)

**Key Components**:

1. **config_db.rs** - Database loading module:
   - `load_config_from_db()`: Loads complete configuration from SQLite
   - Validates API keys with SHA256 hash comparison
   - Loads provider instances with plaintext API keys (required for upstream calls)
   - Falls back to TOML if database is empty

2. **config_api.rs** - REST API handlers:
   - API Keys CRUD: `/api/config/api-keys`
   - Routing Rules CRUD: `/api/config/routing-rules`
   - Provider Instances CRUD: `/api/config/providers/:provider/instances`
   - Hot reload endpoint: `/api/config/reload`

**Critical Implementation Details**:

**API Key Storage Strategy**:
```rust
// Gateway API Keys (for client authentication)
// - Stored as SHA256 hash in database
// - Used for Bearer token validation
// - Hash comparison in config_db.rs

// Provider API Keys (for upstream calls)
// - Stored as PLAINTEXT in database (field name: api_key_encrypted)
// - Required for calling OpenAI/Anthropic/Gemini APIs
// - Cannot be hashed because upstream providers need the actual key
```

**Why Plaintext for Provider Keys?**:
- Gateway must send actual API key to upstream providers
- Hashing is irreversible - cannot recover original key
- Field named `api_key_encrypted` for future encryption implementation
- Current implementation: plaintext storage (security trade-off for functionality)

**Hot Reload Mechanism**:
```rust
// In config_api.rs handlers
async fn create_provider_instance(...) {
    // 1. Insert into database
    sqlx::query("INSERT INTO provider_instances ...").execute(&pool).await?;

    // 2. Reload config from database
    let new_config = config_db::load_config_from_db(&pool).await?;

    // 3. Rebuild load balancers with new config
    let new_load_balancers = build_load_balancers(&new_config);

    // 4. Atomic swap (Arc::new + store)
    app_state.load_balancers.store(Arc::new(new_load_balancers));

    // 5. No server restart required!
}
```

**Configuration Flow**:
```
First Run:
  config.toml → SQLite database → Runtime config

Subsequent Runs:
  SQLite database → Runtime config (TOML ignored)

Web UI Changes:
  UI → REST API → Database → Hot reload → Runtime config
```

**Frontend Components** (`frontend/src/components/config/`):
- `ApiKeysList.vue`: Manage gateway API keys
- `RoutingRulesList.vue`: Configure model routing
- `ProviderInstancesList.vue`: Manage provider backends
- `CreateApiKeyModal.vue`: Create new API key with validation
- `CreateRoutingRuleModal.vue`: Create routing rule
- `CreateProviderInstanceModal.vue`: Create provider instance (with Anthropic-specific fields)

**Important Notes**:
- Database file: `./data/config.db` (back up regularly)
- Migrations run automatically on startup
- Provider instances support Anthropic-specific `extra_config` (api_version, cache settings)
- All changes logged with structured tracing

### Handlers

**Directory**: `src/handlers/`

The gateway provides two API formats through different handlers:

**OpenAI-compatible API**:
- `chat_completions.rs` - `/v1/chat/completions` (works with all providers via protocol conversion)
- `models.rs` - `/v1/models` (model listing)

**Native Provider APIs**:
- `messages.rs` - `/v1/messages` (native Anthropic Messages API, direct passthrough)

**Infrastructure**:
- `health.rs` - `/health`, `/ready` endpoints

**Configuration Management** (NEW in v0.5.0):
- `config_api.rs` - REST API for managing configuration
  - API Keys CRUD: Create, list, update (enable/disable), delete
  - Routing Rules CRUD: Create, list, update, delete
  - Provider Instances CRUD: Create, list, update, delete
  - Hot reload: `/api/config/reload` triggers configuration reload from database
  - All operations update SQLite database and trigger hot reload

**Important Pattern**: All handlers use `execute_with_session()` from `retry.rs` to integrate with load balancing and metrics.

**When to use which API**:
- Use `/v1/chat/completions` for:
  - Multi-provider support in one codebase
  - OpenAI-compatible tools (Cursor, Continue, etc.)
  - Switching between providers without code changes
- Use `/v1/messages` for:
  - Claude Code and official Anthropic SDKs
  - Maximum compatibility with Anthropic-specific features
  - Avoiding protocol conversion overhead

### Frontend Dashboard (NEW in v0.4.0)

**Directory**: `frontend/`

**Stack**: Vue 3 + TypeScript + Vite + Chart.js + Tailwind CSS

**Features**:
- **Real-time Token Usage Charts**: Visualize token consumption over time
- **Provider Health Monitoring**: Track instance health status and failover events
- **API Key Analytics**: Per-key token usage and cost estimation
- **Trace Timeline**: Visualize request traces (spans) with performance breakdown
- **Cost Calculator**: Estimate costs based on token usage and caching
- **Configuration Management** (NEW in v0.5.0): Web UI for managing gateway configuration

**Key Components** (`frontend/src/components/`):
- `dashboard/TokenUsageTimeseries.vue`: Time-series token usage chart
- `dashboard/TokenUsageByApiKey.vue`: Per-key token breakdown
- `dashboard/TokenUsageByInstance.vue`: Per-instance token distribution
- `dashboard/InstanceHealthTimeseries.vue`: Health status over time
- `dashboard/ProviderHealthChart.vue`: Current provider health matrix
- `trace/TraceTimeline.vue`: Request trace visualization
- **`config/ApiKeysList.vue`**: Manage gateway API keys (NEW)
- **`config/RoutingRulesList.vue`**: Configure routing rules (NEW)
- **`config/ProviderInstancesList.vue`**: Manage provider instances (NEW)
- **`config/CreateApiKeyModal.vue`**: Create new API key modal (NEW)
- **`config/CreateRoutingRuleModal.vue`**: Create routing rule modal (NEW)
- **`config/CreateProviderInstanceModal.vue`**: Create provider instance modal (NEW)

**API Endpoints**:
- `GET /api/requests/time-series`: Token usage time series
- `GET /api/requests/by-api-key`: Per-key aggregation
- `GET /api/requests/by-instance`: Per-instance aggregation
- `GET /api/instances/health-time-series`: Instance health over time
- `GET /api/instances/current-health`: Current health status
- **`GET /api/config/api-keys`**: List all API keys (NEW)
- **`POST /api/config/api-keys`**: Create new API key (NEW)
- **`PUT /api/config/api-keys/:name`**: Update API key (NEW)
- **`DELETE /api/config/api-keys/:name`**: Delete API key (NEW)
- **`GET /api/config/routing-rules`**: List routing rules (NEW)
- **`POST /api/config/routing-rules`**: Create routing rule (NEW)
- **`PUT /api/config/routing-rules/:id`**: Update routing rule (NEW)
- **`DELETE /api/config/routing-rules/:id`**: Delete routing rule (NEW)
- **`GET /api/config/providers/:provider/instances`**: List provider instances (NEW)
- **`POST /api/config/providers/:provider/instances`**: Create provider instance (NEW)
- **`PUT /api/config/providers/:provider/instances/:name`**: Update provider instance (NEW)
- **`DELETE /api/config/providers/:provider/instances/:name`**: Delete provider instance (NEW)
- **`POST /api/config/reload`**: Hot reload configuration (NEW)

**Development**:
```bash
cd frontend
npm install
npm run dev        # Development server on http://localhost:3000
npm run build      # Production build
```

**Deployment**:
- Frontend serves from `frontend/dist/` via backend's static file handler
- Access at `http://localhost:8080/` (root path)
- API routes under `/api/*` are proxied to backend

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

### Observability Configuration

```toml
[observability]
enabled = true
database_path = "./data/observability.db"

# Performance tuning
[observability.performance]
batch_size = 100              # Events per batch write
flush_interval_ms = 100       # Max time before flushing
max_buffer_size = 10000       # Ring buffer size

# Data retention policies (automatic cleanup)
[observability.retention]
logs_days = 7                     # Keep request logs for 7 days
spans_days = 7                    # Keep trace spans for 7 days
cleanup_hour = 3                  # Run cleanup at 3 AM daily (0-23)
```

**Key Configuration Options**:
- `batch_size`: Larger = more throughput, higher memory
- `flush_interval_ms`: Smaller = more real-time, more writes
- `max_buffer_size`: Ring buffer prevents blocking on backpressure
- Retention policies: Balance storage vs historical analysis needs

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
