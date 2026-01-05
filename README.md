# LLM Gateway

A high-performance, minimal LLM proxy gateway written in Rust that provides a unified OpenAI-compatible API for multiple LLM providers (OpenAI, Anthropic Claude, Google Gemini).

## Features

- **Unified OpenAI API**: Single `/v1/chat/completions` endpoint supports all providers
- **Protocol Conversion**: Automatic request/response translation between OpenAI, Anthropic, and Gemini formats
- **Smart Routing**: Model-based routing to appropriate providers
- **Zero Dependencies**: No database, Redis, or cache required - just binary + config file
- **Static Authentication**: API key-based auth configured in TOML
- **Prometheus Metrics**: Four-dimension metrics (API key, model, provider, endpoint)
- **Streaming Support**: Full SSE support with real-time protocol conversion
- **Cloud Native**: Docker ready, health checks, structured JSON logging

## Architecture

```
┌─────────────┐
│   Cursor    │  (OpenAI endpoint)
│ Claude Code │  → /v1/chat/completions → Gateway → Auto-routes to:
│   etc.      │                                    ├─ OpenAI (direct)
└─────────────┘                                    ├─ Anthropic (converted)
                                                   └─ Gemini (converted)
```

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
