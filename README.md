# LLM Gateway

A high-performance LLM proxy gateway written in Rust that provides multiple API formats for LLM providers (OpenAI, Anthropic Claude, Google Gemini, Azure OpenAI, AWS Bedrock, and custom OpenAI-compatible services):
- **Unified OpenAI-compatible API** (`/v1/chat/completions`) - works with all providers via automatic protocol conversion (prefix routing)
- **Native Anthropic Messages API** (`/v1/messages`) - direct passthrough for Claude models without conversion overhead
- **Path-Routed Endpoints** - Direct provider access: Azure (`/azure/*`), Bedrock (`/bedrock/*`), Responses API (`/v1/responses`), Custom (`/custom/:id/*`)
- **Trait-based Provider Architecture** - Pluggable provider system via `LlmProvider` + `ProviderConfig` traits, add new providers without modifying match arms
- **Advanced Failover System** - Circuit breaker, exponential backoff, intelligent error classification
- **SQLite-based Observability** - Complete request logging with token tracking and performance metrics

## Features

- **Multiple API Formats**:
  - Unified OpenAI-compatible API (`/v1/chat/completions`) with automatic protocol conversion
  - Native Anthropic Messages API (`/v1/messages`) for direct Claude access
  - Path-routed endpoints for direct provider access (Azure, Bedrock, Custom, Responses API)
- **7 Provider Implementations**:
  - **OpenAI** - Standard chat completions with Bearer auth
  - **Anthropic** - Native messages API with x-api-key/OAuth auth
  - **Google Gemini** - generateContent API with query param/OAuth auth
  - **Azure OpenAI** - Azure-specific URL/auth (`api-key` header, deployment-based routing)
  - **AWS Bedrock** - SigV4 signed requests with model ID mapping
  - **OpenAI Responses API** - `/v1/responses` endpoint passthrough
  - **Custom OpenAI-compatible** - Any OpenAI-compatible service with custom headers
- **Protocol Conversion**: Automatic request/response translation between OpenAI, Anthropic, and Gemini formats
- **Dual Routing Modes**:
  - **Prefix routing**: ModelRouter matches model name prefix to provider (e.g. `"gpt-"` → OpenAI)
  - **Path routing**: URL determines provider directly (e.g. `/azure/v1/chat/completions` → Azure OpenAI)
- **Multi-Instance Load Balancing**: Each provider supports multiple backend instances with priority-based selection
- **Sticky Sessions**: API key-level session affinity maximizes provider-side KV cache hits
- **Advanced Failover System**:
  - **Circuit Breaker**: 3 failures trigger circuit open, half-open state for testing recovery
  - **Intelligent Error Classification**: 401/403 auth errors, 429 rate limits, 503 transient errors handled differently
  - **Exponential Backoff**: 60s → 120s → 240s → 480s → 600s with ±20% jitter
  - **Automatic Retry**: Smart retry logic with max 3 attempts, different strategies per error type
  - **Health Monitoring**: Real-time health status via `stats` command
  - **Event Logging**: failover_events table tracks all circuit breaker state transitions
- **SQLite-based Observability**:
  - Complete request logging with token usage tracking
  - Anthropic prompt caching metrics (cache creation/read tokens)
  - **Automatic Cost Calculation**: Real-time cost tracking with hourly pricing updates
  - Automatic data retention policies (7-30 days)
  - Non-blocking async batch writes
  - **Provider Quota Monitoring**: Automatic quota refresh for Anthropic OAuth instances
- **Flexible Configuration**:
  - TOML-based configuration with hot reload via SIGHUP
  - Dual authentication: Gateway API keys + Provider-specific auth (Bearer, OAuth, SigV4)
- **Streaming Support**: Full SSE support with real-time protocol conversion
- **Cloud Native**: Docker ready, health checks, structured JSON logging
- **Horizontal Scaling**: Nginx-compatible for multi-machine deployments

## Architecture

The gateway provides three routing modes:

```
┌─────────────────────────────────────────────────────────────────────┐
│  Mode 1: Prefix Routing (ModelRouter selects provider by model name) │
│  ┌──────────────┐                                                    │
│  │   Cursor     │                                                    │
│  │  Continue    │  → /v1/chat/completions → ModelRouter →            │
│  │   etc.       │                          ├─ OpenAI (direct)        │
│  └──────────────┘                          ├─ Anthropic (convert)    │
│                                             └─ Gemini (convert)      │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  Mode 2: Native API (dedicated provider endpoint)                    │
│  ┌──────────────┐                                                    │
│  │ Claude Code  │  → /v1/messages → Gateway → Anthropic              │
│  │  Anthropic   │                   (native format, no conversion)   │
│  │    SDK       │                                                    │
│  └──────────────┘                                                    │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  Mode 3: Path Routing (URL determines provider, bypasses router)     │
│  ┌──────────────┐  → /azure/v1/chat/completions → Azure OpenAI      │
│  │  Direct      │  → /bedrock/v1/messages       → AWS Bedrock        │
│  │  Provider    │  → /v1/responses              → OpenAI Responses   │
│  │  Access      │  → /custom/:id/v1/chat/compl. → Custom Provider    │
│  └──────────────┘                                                    │
└─────────────────────────────────────────────────────────────────────┘
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

# Gateway API Keys
[[api_keys]]
key = "sk-gateway-001"
name = "my-app"
enabled = true

# Model Routing (prefix matching)
[routing]
default_provider = "openai"

[routing.rules]
"gpt-" = "openai"
"claude-" = "anthropic"
"gemini-" = "gemini"
"deepseek-" = "custom:deepseek"    # Custom provider prefix routing

# Provider Configurations (each is an array of instances)
[[providers.openai]]
name = "openai-primary"
enabled = true
api_key = "sk-your-openai-key"
base_url = "https://api.openai.com/v1"
timeout_seconds = 300
priority = 1

[[providers.anthropic]]
name = "anthropic-primary"
enabled = true
api_key = "sk-ant-your-key"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"
priority = 1

[[providers.gemini]]
name = "gemini-primary"
enabled = true
api_key = "your-gemini-key"
base_url = "https://generativelanguage.googleapis.com/v1beta"
timeout_seconds = 300
priority = 1

# Azure OpenAI (path-routed via /azure/v1/chat/completions)
[[providers.azure_openai]]
name = "azure-east"
enabled = true
api_key = "your-azure-key"
resource_name = "my-openai-resource"
api_version = "2024-02-01"
timeout_seconds = 300
priority = 1

[providers.azure_openai.model_deployments]
"gpt-4" = "gpt-4-deployment"

# AWS Bedrock (path-routed via /bedrock/v1/messages)
[[providers.bedrock]]
name = "bedrock-east"
enabled = true
region = "us-east-1"
access_key_id = "AKIA..."
secret_access_key = "..."
timeout_seconds = 300
priority = 1

[providers.bedrock.model_id_mapping]
"claude-3-5-sonnet" = "anthropic.claude-3-5-sonnet-20241022-v2:0"

# Custom OpenAI-compatible (path-routed via /custom/deepseek/v1/chat/completions)
[[providers.custom]]
name = "deepseek-primary"
enabled = true
provider_id = "deepseek"
api_key = "sk-..."
base_url = "https://api.deepseek.com/v1"
timeout_seconds = 300
priority = 1
```

### 2. Run with Docker

```bash
docker build -t llm-gateway .
docker run -p 8080:8080 -v $(pwd)/config.toml:/app/config.toml llm-gateway
```

### 3. Run from source

```bash
cd backend
cargo build --release
./target/release/llm-gateway start
```

## API Endpoints

### Prefix-Routed APIs (ModelRouter selects provider)

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/chat/completions` | POST | Yes | OpenAI-compatible chat completion (all providers) |
| `/v1/messages` | POST | Yes | Native Anthropic Messages API (Claude models only) |
| `/v1/models` | GET | Yes | List available models |
| `/v1beta/models` | GET | Yes | Gemini native: list models |
| `/v1beta/models/*` | GET/POST | Yes | Gemini native: get model / generate content |

### Path-Routed APIs (URL determines provider)

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/azure/v1/chat/completions` | POST | Yes | Azure OpenAI direct access |
| `/bedrock/v1/messages` | POST | Yes | AWS Bedrock direct access (Anthropic format) |
| `/v1/responses` | POST | Yes | OpenAI Responses API passthrough |
| `/custom/:provider_id/v1/chat/completions` | POST | Yes | Custom OpenAI-compatible provider |

### Monitoring

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/health` | GET | No | Health check |
| `/ready` | GET | No | Readiness check |

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

**Prefix routing** (ModelRouter selects provider by model name):

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-3-5-sonnet", "messages": [{"role": "user", "content": "Hello!"}]}'
```

**Native Anthropic API** (Claude only, no conversion):

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-3-5-sonnet-20241022", "max_tokens": 1024,
       "messages": [{"role": "user", "content": "Hello!"}]}'
```

**Path-routed: Azure OpenAI**:

```bash
curl -X POST http://localhost:8080/azure/v1/chat/completions \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hello!"}]}'
```

**Path-routed: AWS Bedrock** (Anthropic messages format):

```bash
curl -X POST http://localhost:8080/bedrock/v1/messages \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-3-5-sonnet", "max_tokens": 1024,
       "messages": [{"role": "user", "content": "Hello!"}]}'
```

**Path-routed: Custom provider** (e.g. DeepSeek):

```bash
curl -X POST http://localhost:8080/custom/deepseek/v1/chat/completions \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "deepseek-chat", "messages": [{"role": "user", "content": "Hello!"}]}'
```

**OpenAI Responses API**:

```bash
curl -X POST http://localhost:8080/v1/responses \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "input": "Hello!"}'
```

## Observability

### SQLite-based Observability

All requests are logged to SQLite database (`./data/observability.db`) with complete details:

**Request Data Includes**:
- Basic info: request_id, timestamp, api_key_name, provider, instance, model, endpoint
- Token usage: input_tokens, output_tokens, total_tokens
- **Caching metrics**: cache_creation_input_tokens, cache_read_input_tokens (Anthropic only)
- **Cost breakdown**: input_cost, output_cost, cache_write_cost, cache_read_cost, total_cost
- Performance: duration_ms, status, error_type, error_message

**Provider Quota Monitoring** (NEW):

The gateway automatically monitors token quotas for Anthropic OAuth instances. View quota status with `llm-gateway stats`.

**Supported Providers**:
- ✅ **Anthropic (OAuth mode only)**: Supports quota queries via Anthropic's OAuth usage API
- ❌ **Anthropic (Bearer mode)** / **OpenAI** / **Gemini**: Not supported (no public API available)

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

**Configuration**:

```toml
[observability]
enabled = true
database_path = "./data/observability.db"

# Quota refresh configuration
[observability.quota_refresh]
enabled = true              # Enable/disable quota monitoring
interval_seconds = 600      # Refresh interval (default: 10 minutes)
```

**Data Retention**:
- Request logs: 7 days (configurable)
- Quota snapshots: 7 days (configurable)
- Automatic cleanup runs daily at 3 AM

For SQL query examples and advanced observability configuration, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md#observability-internals).

## Feature Matrix

### Provider Support

| Provider | Protocol | Auth | Routing | Streaming |
|----------|----------|------|---------|-----------|
| **OpenAI** | OpenAI | Bearer | Prefix (`gpt-`) | SSE |
| **Anthropic** | Anthropic | x-api-key / OAuth | Prefix (`claude-`) + `/v1/messages` | SSE |
| **Gemini** | Gemini | Query param / OAuth | Prefix (`gemini-`) + `/v1beta/*` | SSE |
| **Azure OpenAI** | OpenAI | `api-key` header | Path (`/azure/*`) | SSE |
| **AWS Bedrock** | Anthropic | AWS SigV4 | Path (`/bedrock/*`) | AWS Event Stream |
| **OpenAI Responses** | OpenAI | Bearer | Path (`/v1/responses`) | SSE |
| **Custom** | OpenAI | Bearer + custom headers | Path (`/custom/:id/*`) + Prefix | SSE |

### Multimodal Features (via `/v1/chat/completions`)

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

## Configuration

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
3. Parse the output and send alerts when utilization exceeds a threshold

Example cron job:
```bash
# Check quota every hour and alert if >80%
0 * * * * /path/to/llm-gateway stats | grep -q "5h: 8[0-9]\." && echo "High quota usage!" | mail -s "Alert" admin@example.com
```

## Development & Architecture

For development workflows (building, testing, cross-compilation), internal architecture details (trait-based provider system, circuit breaker, protocol conversion), and advanced configuration, see **[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)**.

Additional documentation:
- **[docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md)** - Complete implementation details
- **[docs/FEATURES.md](docs/FEATURES.md)** - Comprehensive feature documentation
- **[docs/CONVERSION_LIMITATIONS.md](docs/CONVERSION_LIMITATIONS.md)** - Provider conversion trade-offs
- **[docs/DAEMON.md](docs/DAEMON.md)** - Running as a daemon/background service

Built with Rust using Axum, Tokio, and SQLite.
