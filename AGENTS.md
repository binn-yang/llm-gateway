# AGENTS.md

Guidelines for agentic coding agents working on the LLM Gateway codebase.

## Project Overview

LLM Gateway is a high-performance Rust proxy service providing unified APIs for multiple LLM providers (OpenAI, Anthropic, Gemini). Key features: priority-based sticky session load balancing, automatic failover, SQLite observability, and complete token tracking.

**Tech Stack**: Rust + Axum + Tokio + SQLite (backend), Vue 3 + TypeScript + Chart.js (frontend)

## Build Commands

```bash
cargo build              # Development build
cargo build --release    # Production build (optimized)
cargo run --release      # Run the service
./target/release/llm-gateway test   # Test configuration
./target/release/llm-gateway start  # Start service
```

## Test Commands

```bash
cargo test                           # Run all tests
cargo test test_name_substring       # Run specific test (substring match)
cargo test --test stress_scenarios   # Run specific test file
cargo test --lib load_balancer       # Run tests in a module
cargo test --lib -- exact_test_name  # Run single test (exact match)
cargo test -- --nocapture            # Run with verbose output
cargo test -- --ignored              # Run ignored tests
```

## Lint Commands

```bash
cargo check        # Check for compilation errors (fast)
cargo clippy       # Check with clippy linter
cargo fmt          # Format code
cargo fmt -- --check  # Format check (CI mode)
```

## Code Style Guidelines

### Imports

Group imports in order: 1) `crate::` internal, 2) External crates, 3) Standard library.

```rust
use crate::{
    auth::AuthInfo,
    error::AppError,
    models::anthropic::MessagesRequest,
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{Timelike, Utc};
use std::time::Instant;
```

### Formatting

- Use `cargo fmt` before committing
- Max line length: 100 characters
- Indent: 4 spaces
- Use trailing commas in multi-line arrays/structs

### Types and Models

Use `serde_json::Value` for passthrough fields from external APIs:

```rust
pub struct ContentBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,  // Accept any format
}
```

- `Value`: Passthrough fields, upstream-validated fields, external client data
- Strong types: Fields the gateway reads/modifies, fields with business logic

### Naming Conventions

- **Files**: `snake_case.rs` (e.g., `load_balancer.rs`)
- **Structs/Enums**: `PascalCase` (e.g., `LoadBalancer`, `AppError`)
- **Functions**: `snake_case` (e.g., `handle_messages`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `FAILURE_THRESHOLD`)

### Error Handling

Use `AppError` enum from `src/error.rs`:

```rust
AppError::ConfigError("message".to_string())
AppError::UpstreamError { status: StatusCode::BAD_GATEWAY, message: "msg".to_string() }
AppError::RateLimitError { provider: "openai".into(), instance: "primary".into(), 
    retry_after: Some(60), message: "Rate limited".into() }
AppError::NoHealthyInstances("All instances unhealthy".to_string())
```

**Failover error classification** (`src/retry.rs`):
- `401/403` → Instance failure | `429` → Rate limit | `503` → Transient
- `500/502/504` → Instance failure | `4xx` → Business error (no failover)

### Logging

Use structured logging with `tracing`:

```rust
tracing::info!(request_id = %request_id, model = %model, "Request completed");
tracing::error!(error = %e, "Failed to deserialize request");
let span = tracing::info_span!("request", request_id = %request_id, model = %model);
```

### Async Patterns

- `tokio::spawn` for fire-and-forget background tasks
- `Arc` for shared state across async boundaries
- `DashMap` for low-contention concurrent maps
- `RwLock` for read-heavy workloads

## Project Structure

```
backend/src/
├── handlers/      # HTTP request handlers
├── providers/     # Provider API clients (openai, anthropic, gemini)
├── converters/    # Protocol converters (OpenAI ↔ Anthropic ↔ Gemini)
├── models/        # Data models for each protocol
├── oauth/         # OAuth authentication system
├── observability/ # Request logging and metrics
├── pricing/       # Cost calculation system
├── retry.rs       # Failover and retry logic
├── load_balancer.rs # Session-based load balancing
├── router.rs      # Model routing
└── error.rs       # Application error types
```

## Gateway Design Principles

1. **Forward, don't validate**: The gateway routes and forwards; upstream APIs validate
2. **Official client compatibility**: If official clients fail, the gateway is wrong
3. **Passthrough first**: Use `serde_json::Value` for fields that just need forwarding
4. **No information loss**: Never remove fields we don't understand

## Common Patterns

### Adding a new API endpoint

1. Create handler in `src/handlers/`
2. Add route in `src/server.rs`
3. Add provider call in `src/providers/`
4. Add model types in `src/models/`
5. Add converter if needed in `src/converters/`

### Modifying load balancing

Edit `src/load_balancer.rs` and `src/retry.rs`:
- Priority: lower number = higher priority
- Sticky sessions: API key bound to instance for 1 hour
- Failover: automatic on instance failure

## Security

- Never commit `config.toml` with real API keys
- Request body limit: 10MB
- OAuth tokens encrypted with AES-256-GCM
