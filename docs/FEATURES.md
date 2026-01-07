# LLM Gateway - Feature Documentation

This document provides comprehensive documentation for all features supported by the LLM Gateway.

## Table of Contents

1. [Feature Matrix](#feature-matrix)
2. [Vision & Image Support](#vision--image-support)
3. [Tool/Function Calling](#toolfunction-calling)
4. [Prompt Caching](#prompt-caching)
5. [Structured Outputs (JSON Mode)](#structured-outputs-json-mode)
6. [Streaming Support](#streaming-support)
7. [Provider-Specific Features](#provider-specific-features)
8. [Conversion Warnings](#conversion-warnings)

---

## Feature Matrix

Complete feature support across all providers:

| Feature | OpenAI | Anthropic | Gemini | Implementation |
|---------|:------:|:---------:|:------:|----------------|
| **Basic Text** | ✅ | ✅ | ✅ | Native support for all |
| **Streaming** | ✅ | ✅ | ✅ | SSE with real-time conversion |
| **Vision/Images** | ✅ | ✅ | ✅ | Auto base64 conversion |
| **Tool Calling** | ✅ | ✅ | ✅ | Format conversion |
| **Streaming Tools** | ✅ | ✅ | ✅ | Incremental JSON assembly |
| **Prompt Caching** | ❌ | ✅ | ❌ | Auto-caching logic |
| **JSON Mode** | ✅ | ✅ ⚠️ | ✅ | ⚠️ System prompt workaround |
| **JSON Schema** | ✅ | ✅ ⚠️ | ✅ | ⚠️ System prompt workaround |
| **Thinking Mode** | ❌ | ✅ | ❌ | Anthropic extended thinking |
| **Safety Settings** | ❌ | ❌ | ✅ | Gemini content filtering |
| **Metadata** | ❌ | ✅ | ❌ | Request tracking |

**Legend:**
- ✅ = Full support (native or converted)
- ⚠️ = Supported via workaround
- ❌ = Not available for this provider

---

## Vision & Image Support

### Overview

All three providers support vision/image inputs through the gateway. The gateway handles format conversion automatically.

### Request Format

Use OpenAI's content blocks format:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "What do you see in this image?"
        },
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,/9j/4AAQSkZJRg...",
            "detail": "high"
          }
        }
      ]
    }
  ],
  "max_tokens": 1024
}
```

### Image URL Formats

#### Base64 Data URLs (Recommended)

```
data:image/jpeg;base64,/9j/4AAQSkZJRg...
data:image/png;base64,iVBORw0KGgo...
data:image/gif;base64,R0lGODlh...
data:image/webp;base64,UklGRgAA...
```

#### HTTP URLs

```
https://example.com/image.jpg
```

**Note:** The gateway will fetch HTTP URLs and convert them to base64 for providers that require it (Anthropic, Gemini).

### Detail Levels

The `detail` parameter controls image resolution:

- **`"low"`**: Faster, cheaper, lower resolution (512x512)
- **`"high"`**: Slower, more expensive, full resolution
- **`"auto"`**: Gateway decides based on image size (default)

### Provider-Specific Handling

| Provider | Input Format | Max Size | Supported Types |
|----------|--------------|----------|-----------------|
| **OpenAI** | URL or base64 | N/A | JPEG, PNG, GIF, WebP |
| **Anthropic** | Base64 only | 5 MB | JPEG, PNG, GIF, WebP |
| **Gemini** | Base64 only | 20 MB | PNG, JPEG, WebP, HEIC, HEIF |

The gateway automatically:
1. Fetches HTTP URLs if needed
2. Validates image size
3. Converts to base64 for Anthropic/Gemini
4. Handles MIME type detection

### Multi-Image Requests

Send multiple images in a single request:

```json
{
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "Compare these images:"},
        {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,..."}},
        {"type": "text", "text": "versus"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,..."}}
      ]
    }
  ]
}
```

### Error Handling

Common errors and solutions:

| Error | Cause | Solution |
|-------|-------|----------|
| `Image too large` | Exceeds provider limit | Compress or resize image |
| `Unsupported format` | Invalid MIME type | Use JPEG, PNG, GIF, or WebP |
| `Failed to fetch URL` | Network error or invalid URL | Use base64 data URL instead |
| `Invalid base64` | Malformed data URL | Check encoding |

---

## Tool/Function Calling

### Overview

The gateway supports tool calling (function calling) across all providers with automatic format conversion.

### Request Format

Define tools using OpenAI's format:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {
      "role": "user",
      "content": "What's the weather in San Francisco?"
    }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get the current weather in a location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {
              "type": "string",
              "description": "City and state, e.g. San Francisco, CA"
            },
            "unit": {
              "type": "string",
              "enum": ["celsius", "fahrenheit"],
              "default": "fahrenheit"
            }
          },
          "required": ["location"]
        }
      }
    }
  ],
  "tool_choice": "auto"
}
```

### Tool Choice Modes

| Mode | OpenAI | Anthropic | Gemini | Behavior |
|------|--------|-----------|--------|----------|
| `"auto"` | ✅ | ✅ (auto) | ✅ (AUTO) | Model decides when to use tools |
| `"required"` | ✅ | ✅ (any) | ✅ (ANY) | Model must use a tool |
| `{"type":"function","function":{"name":"..."}}` | ✅ | ✅ (tool) | ✅ (allowedFunctionNames) | Force specific tool |
| `"none"` | ✅ | N/A | ✅ (NONE) | Disable tools |

### Provider Conversion

#### OpenAI → Anthropic

```json
// OpenAI format
{
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "parameters": {...}
    }
  }]
}

// Converted to Anthropic
{
  "tools": [{
    "name": "get_weather",
    "description": "...",
    "input_schema": {...}
  }]
}
```

#### OpenAI → Gemini

```json
// OpenAI format
{
  "tools": [...]
}

// Converted to Gemini
{
  "tools": [{
    "function_declarations": [{
      "name": "get_weather",
      "description": "...",
      "parameters": {...}
    }]
  }]
}
```

### Response Format

Tool use in assistant response:

```json
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": "I'll check the weather for you.",
      "tool_calls": [{
        "id": "call_abc123",
        "type": "function",
        "function": {
          "name": "get_weather",
          "arguments": "{\"location\":\"San Francisco, CA\",\"unit\":\"fahrenheit\"}"
        }
      }]
    },
    "finish_reason": "tool_calls"
  }]
}
```

### Multi-Turn with Tool Results

Send tool results back to continue the conversation:

```json
{
  "messages": [
    {"role": "user", "content": "What's the weather?"},
    {
      "role": "assistant",
      "content": "I'll check for you.",
      "tool_calls": [...]
    },
    {
      "role": "tool",
      "name": "get_weather",
      "content": [
        {
          "type": "tool_result",
          "tool_call_id": "call_abc123",
          "content": "{\"temperature\":72,\"conditions\":\"sunny\"}"
        }
      ]
    }
  ]
}
```

### Streaming Tool Calls

With `"stream": true`, tool calls are delivered incrementally:

```
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"location\""}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"SF\""}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}}]}

data: [DONE]
```

The gateway assembles incremental `input_json_delta` events from Anthropic into OpenAI's `tool_calls` format.

### Best Practices

1. **Clear Descriptions**: Tool and parameter descriptions help the model choose correctly
2. **Type Constraints**: Use JSON schema validation (enum, minimum, maximum)
3. **Required Fields**: Mark essential parameters as required
4. **Error Handling**: Return error messages in tool results when execution fails
5. **Streaming**: Use streaming for long-running tool operations to show progress

---

## Prompt Caching

### Overview

Anthropic's prompt caching feature reduces costs by ~90% for repeated content. The gateway implements **auto-caching** logic.

### Configuration

Enable auto-caching in `config.toml`:

```toml
[[providers.anthropic]]
name = "anthropic-primary"
enabled = true
api_key = "sk-ant-..."
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"
priority = 1
failure_timeout_seconds = 60

[providers.anthropic.cache]
auto_cache_system = true          # Auto-cache large system prompts
min_system_tokens = 1024           # Minimum tokens to trigger caching
auto_cache_tools = true            # Auto-cache tool definitions
```

### How Auto-Caching Works

The gateway automatically applies caching when:

1. **Large System Prompts**: System prompt ≥ `min_system_tokens` (default 1024)
   - Converts Text → Blocks format
   - Adds `cache_control` to last block

2. **Tool Definitions**: When `auto_cache_tools = true`
   - Adds `cache_control` to last tool in the array

### Token Estimation

The gateway estimates tokens using: **1 token ≈ 4 characters**

Example:
- System prompt: "You are a helpful assistant. " × 300 = 8,400 characters
- Estimated tokens: 8,400 / 4 = 2,100 tokens
- Triggers auto-caching (> 1024 threshold)

### Cache Behavior

| Metric | First Request | Cache Hit | Savings |
|--------|---------------|-----------|---------|
| **Input Tokens** | 5,000 | 100 | - |
| **Cache Write** | 5,000 @ $3.75/MTok | - | - |
| **Cache Read** | - | 5,000 @ $0.30/MTok | 90% |
| **Total Cost** | $0.01875 | $0.00150 | 92% |

**Cache Lifetime**: 5 minutes (ephemeral)

### Usage Tracking

Monitor cache metrics in response usage:

```json
{
  "usage": {
    "input_tokens": 100,
    "output_tokens": 500,
    "cache_creation_input_tokens": 5000,    // First request
    "cache_read_input_tokens": 5000          // Subsequent requests
  }
}
```

### Break-Even Analysis

```
First request: $0.01875 (cache write)
Each subsequent: $0.00150 (cache read)

Break-even: 2 requests
10 requests: $0.11025 (vs $0.25050 without caching)
Savings: 56%
```

### Best Practices

1. **Consistent System Prompts**: Keep system prompts identical across requests
2. **Reuse Tool Definitions**: Don't modify tool schemas unnecessarily
3. **Request Frequency**: Make follow-up requests within 5 minutes
4. **Monitor Usage**: Track `cache_creation_input_tokens` and `cache_read_input_tokens`

### Disabling Auto-Caching

Set `auto_cache_system = false` or `auto_cache_tools = false` to disable:

```toml
[providers.anthropic.cache]
auto_cache_system = false
auto_cache_tools = false
```

---

## Structured Outputs (JSON Mode)

### Overview

Request structured JSON outputs using `response_format`. The gateway handles provider-specific implementations.

### JSON Object Mode

Request any valid JSON object:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{
    "role": "user",
    "content": "List three colors as JSON"
  }],
  "response_format": {
    "type": "json_object"
  }
}
```

**Provider Implementation:**
- **OpenAI**: Native `response_format` support
- **Gemini**: Native via `response_mime_type: "application/json"`
- **Anthropic**: System prompt injection (check `X-LLM-Gateway-Warnings`)

### JSON Schema Mode

Enforce strict schema compliance:

```json
{
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "color_list",
      "description": "List of colors",
      "strict": true,
      "schema": {
        "type": "object",
        "properties": {
          "colors": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "name": {"type": "string"},
                "hex": {"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"}
              },
              "required": ["name", "hex"],
              "additionalProperties": false
            }
          }
        },
        "required": ["colors"],
        "additionalProperties": false
      }
    }
  }
}
```

**Provider Implementation:**
- **OpenAI**: Native `json_schema` with strict mode
- **Gemini**: Native via `response_schema`
- **Anthropic**: System prompt with schema description (best effort)

### Anthropic Workaround

For Anthropic, the gateway injects JSON instructions into the system prompt:

```
Original system: "You are a helpful assistant."

Modified system: "You are a helpful assistant.

IMPORTANT: You must respond with valid JSON matching this schema:
{schema details}

Do not include any text outside the JSON object."
```

**Check for workarounds:**

```http
X-LLM-Gateway-Warnings: [{"level":"warning","message":"JSON mode implemented via system prompt injection for Anthropic"}]
```

### Schema Validation

The gateway supports full JSON Schema Draft 7:

- **Type constraints**: string, number, integer, boolean, array, object, null
- **String constraints**: minLength, maxLength, pattern, format
- **Number constraints**: minimum, maximum, multipleOf
- **Array constraints**: minItems, maxItems, uniqueItems
- **Object constraints**: required, additionalProperties, properties
- **Enums**: Restrict values to specific options

### Combining with Vision

JSON mode works with images:

```json
{
  "messages": [{
    "role": "user",
    "content": [
      {"type": "text", "text": "Extract objects from this image as JSON"},
      {"type": "image_url", "image_url": {"url": "..."}}
    ]
  }],
  "response_format": {"type": "json_object"}
}
```

### Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| `Invalid JSON schema` | Malformed schema | Validate schema structure |
| `Schema validation failed` | Response doesn't match schema | Review schema constraints |
| `Strict mode not supported` | Provider limitation | Use flexible schema or change provider |

---

## Streaming Support

### Overview

All providers support Server-Sent Events (SSE) streaming with real-time protocol conversion.

### Request Format

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [...],
  "stream": true
}
```

### Response Format

```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"claude-3-5-sonnet-20241022","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"claude-3-5-sonnet-20241022","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"claude-3-5-sonnet-20241022","choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"claude-3-5-sonnet-20241022","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":25,"total_tokens":35}}

data: [DONE]
```

### Event Conversion

The gateway converts provider-specific streaming formats:

#### Anthropic Events

| Anthropic Event | OpenAI Chunk | Contains |
|-----------------|--------------|----------|
| `message_start` | First chunk | Role |
| `content_block_start` | Tool start chunk | Tool ID, name |
| `content_block_delta` (text) | Content chunk | Incremental text |
| `content_block_delta` (json) | Tool args chunk | Partial JSON |
| `message_delta` | Finish chunk | Stop reason |
| `message_stop` | N/A | End signal |

#### Gemini Events

| Gemini Event | OpenAI Chunk | Contains |
|--------------|--------------|----------|
| First response | Role chunk | Role |
| Candidates delta | Content chunk | Text |
| Final response | Finish chunk | Stop reason, usage |

### Streaming with Tools

Tool calls are streamed incrementally:

```
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"get_weather"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\""}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"location"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\":\"SF\"}"}}]}}]}
```

### Buffer Management

The gateway uses efficient buffering:
- Line-based parsing for SSE format
- Incremental JSON assembly for tool args
- Memory-safe streaming with backpressure

### Error Handling in Streams

```
data: {"error":{"message":"Rate limit exceeded","type":"rate_limit_error","code":"rate_limit"}}

data: [DONE]
```

---

## Provider-Specific Features

### Anthropic Extended Thinking

Enable extended thinking mode:

```json
{
  "thinking": {
    "type": "enabled",
    "budget_tokens": 10000
  }
}
```

Response includes thinking blocks:

```json
{
  "content": [
    {
      "type": "thinking",
      "thinking": "Let me analyze this step by step..."
    },
    {
      "type": "text",
      "text": "Based on my analysis..."
    }
  ]
}
```

### Anthropic Request Metadata

Track requests with metadata:

```json
{
  "metadata": {
    "user_id": "user-123"
  }
}
```

### Gemini Safety Settings

Configure content filtering:

```json
{
  "safety_settings": [
    {
      "category": "HARM_CATEGORY_HARASSMENT",
      "threshold": "BLOCK_MEDIUM_AND_ABOVE"
    },
    {
      "category": "HARM_CATEGORY_HATE_SPEECH",
      "threshold": "BLOCK_ONLY_HIGH"
    }
  ]
}
```

Categories:
- `HARM_CATEGORY_HARASSMENT`
- `HARM_CATEGORY_HATE_SPEECH`
- `HARM_CATEGORY_SEXUALLY_EXPLICIT`
- `HARM_CATEGORY_DANGEROUS_CONTENT`

Thresholds:
- `BLOCK_NONE`
- `BLOCK_ONLY_HIGH`
- `BLOCK_MEDIUM_AND_ABOVE`
- `BLOCK_LOW_AND_ABOVE`

---

## Conversion Warnings

### Overview

The gateway adds HTTP headers to inform about unsupported parameters or workarounds.

### Header Format

```http
X-LLM-Gateway-Warnings: [{"level":"warning","message":"Parameter 'seed' not supported by Anthropic provider, ignoring"}]
```

### Warning Triggers

Parameters that generate warnings:

| Parameter | OpenAI | Anthropic | Gemini | Warning |
|-----------|:------:|:---------:|:------:|---------|
| `seed` | ✅ | ❌ | ❌ | "Parameter 'seed' not supported" |
| `logprobs` | ✅ | ❌ | ❌ | "Parameter 'logprobs' not supported" |
| `top_logprobs` | ✅ | ❌ | ❌ | "Parameter 'top_logprobs' not supported" |
| `logit_bias` | ✅ | ❌ | ❌ | "Parameter 'logit_bias' not supported" |
| `service_tier` | ✅ | ❌ | ❌ | "Parameter 'service_tier' not supported" |
| `presence_penalty` | ✅ | ❌ | ❌ | "Parameter 'presence_penalty' not supported" |
| `frequency_penalty` | ✅ | ❌ | ❌ | "Parameter 'frequency_penalty' not supported" |
| `n > 1` | ✅ | ❌ | ❌ | "Multiple completions (n > 1) not supported" |
| `response_format` (Anthropic) | ✅ | ⚠️ | ✅ | "JSON mode implemented via system prompt injection" |

### Reading Warnings in Code

```javascript
const response = await fetch('http://localhost:8080/v1/chat/completions', {
  method: 'POST',
  headers: {
    'Authorization': 'Bearer sk-gateway-001',
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({...})
});

const warnings = response.headers.get('x-llm-gateway-warnings');
if (warnings) {
  const warningList = JSON.parse(warnings);
  warningList.forEach(w => console.warn(`${w.level}: ${w.message}`));
}
```

### Warning Levels

- **`"warning"`**: Feature not natively supported, workaround applied or ignored
- **`"info"`**: Informational message about conversion

---

## Migration Guide

### From Direct Provider APIs

#### Anthropic → Gateway

```diff
- const response = await anthropic.messages.create({
+ const response = await openaiClient.chat.completions.create({
-   model: "claude-3-5-sonnet-20241022",
+   model: "claude-3-5-sonnet",
-   max_tokens: 1024,
+   max_tokens: 1024,  // Still required
-   messages: [...]
+   messages: [...]    // Same format
  });
```

#### Gemini → Gateway

```diff
- const response = await gemini.generateContent({
+ const response = await openaiClient.chat.completions.create({
-   model: "gemini-1.5-pro",
+   model: "gemini-1.5-pro",
-   contents: [{role: "user", parts: [{text: "..."}]}]
+   messages: [{role: "user", content: "..."}]
  });
```

### From OpenAI API

No changes needed! Just point to the gateway:

```diff
- const openai = new OpenAI({baseURL: "https://api.openai.com/v1"});
+ const openai = new OpenAI({baseURL: "http://localhost:8080/v1"});
```

Change model names to route to different providers:
- `"gpt-4"` → OpenAI
- `"claude-3-5-sonnet"` → Anthropic
- `"gemini-1.5-pro"` → Gemini

---

## Performance Considerations

### Latency

Typical latency overhead:
- **Protocol conversion**: ~1-5ms
- **Image base64 conversion**: ~10-50ms (depending on size)
- **Auto-caching logic**: ~1-2ms

Total overhead: **< 100ms** for most requests

### Throughput

The gateway is built with Rust and Tokio for high performance:
- Async I/O throughout
- Zero-copy where possible
- Efficient JSON parsing with serde
- Minimal allocations

Tested at **1000+ RPS** per instance on standard hardware.

### Memory

Memory usage per request:
- **Small request** (text only): ~10-50 KB
- **Image request**: ~5-10 MB (temporary, released after conversion)
- **Streaming**: ~100 KB buffer per connection

### Scaling

- **Vertical**: Single instance handles 1000+ RPS
- **Horizontal**: Add instances behind Nginx with consistent hashing
- **Provider**: Multi-instance load balancing with sticky sessions

---

## Troubleshooting

### Common Issues

| Issue | Symptom | Solution |
|-------|---------|----------|
| **Wrong provider** | "Model not found" | Check model → provider mapping in config |
| **Image too large** | "File size exceeds limit" | Compress image or use lower resolution |
| **Tool not called** | Model doesn't use tool | Improve tool description, try `tool_choice: "required"` |
| **Cache not working** | No `cache_read_input_tokens` | Ensure auto-caching enabled and prompt > threshold |
| **JSON invalid** | Response not JSON | Check `X-LLM-Gateway-Warnings` for Anthropic workarounds |

### Debug Mode

Enable debug logging:

```toml
[server]
log_level = "debug"
```

View detailed conversion logs:
- Request transformation
- Response conversion
- Auto-caching decisions
- Warning generation

---

For more information, see:
- [CONVERSION_LIMITATIONS.md](CONVERSION_LIMITATIONS.md) - Provider conversion trade-offs
- [README.md](../README.md) - Quick start guide
- [PHASES_COMPLETE.md](../PHASES_COMPLETE.md) - Implementation status
