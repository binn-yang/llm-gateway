# LLM Gateway - Conversion Limitations & Trade-offs

This document outlines what gets lost, modified, or approximated when converting between provider APIs.

## Table of Contents

1. [Overview](#overview)
2. [OpenAI → Anthropic](#openai--anthropic)
3. [OpenAI → Gemini](#openai--gemini)
4. [Anthropic → OpenAI](#anthropic--openai)
5. [Gemini → OpenAI](#gemini--openai)
6. [Best Practices](#best-practices)
7. [Provider Feature Comparison](#provider-feature-comparison)

---

## Overview

The gateway provides a unified OpenAI-compatible API, but each provider has unique features and limitations. This document helps you understand:

- **What's preserved**: Features that convert cleanly
- **What's lost**: Features with no equivalent
- **What's approximated**: Features implemented via workarounds

**Golden Rule**: Use the most-capable provider for the feature you need, or design your application to work with the lowest common denominator.

---

## OpenAI → Anthropic

### Parameters Dropped (With Warnings)

These OpenAI parameters are **ignored** when routing to Anthropic:

| Parameter | OpenAI Support | Anthropic Equivalent | Gateway Behavior |
|-----------|----------------|----------------------|------------------|
| `seed` | Deterministic sampling | ❌ None | Dropped, warning header |
| `logprobs` | Token probabilities | ❌ None | Dropped, warning header |
| `top_logprobs` | Top-k token probs | ❌ None | Dropped, warning header |
| `logit_bias` | Token probability bias | ❌ None | Dropped, warning header |
| `service_tier` | Priority routing | ❌ None | Dropped, warning header |
| `presence_penalty` | Repetition reduction | ❌ None | Dropped, warning header |
| `frequency_penalty` | Frequency reduction | ❌ None | Dropped, warning header |
| `n` (if > 1) | Multiple completions | ❌ None | **Error returned** |

**Warning Header Example:**

```http
X-LLM-Gateway-Warnings: [
  {"level":"warning","message":"Parameter 'seed' not supported by Anthropic provider, ignoring"},
  {"level":"warning","message":"Parameter 'logprobs' not supported by Anthropic provider, ignoring"}
]
```

### JSON Mode Implementation

| OpenAI | Anthropic | Gateway Approach |
|--------|-----------|------------------|
| `response_format: {"type": "json_object"}` | ❌ No native support | ✅ System prompt injection |
| `response_format: {"type": "json_schema"}` | ❌ No native support | ✅ System prompt + schema description |

**System Prompt Injection Example:**

```
Original system: "You are a helpful assistant."

Modified system: "You are a helpful assistant.

IMPORTANT: You must respond with valid JSON only. Do not include any text outside the JSON object. The response must be a valid JSON object."
```

**Trade-offs:**
- ✅ Works reasonably well for simple JSON
- ⚠️ No guaranteed schema compliance (best effort)
- ⚠️ May occasionally return text before/after JSON
- ❌ Strict mode not enforceable

**Recommendation**: Use Gemini or OpenAI for strict JSON schema requirements.

### Temperature Range

| OpenAI | Anthropic | Gateway Behavior |
|--------|-----------|------------------|
| 0.0 - 2.0 | 0.0 - 1.0 | **Clips to 1.0** if > 1.0 |

Example:
```
Request: temperature = 1.5
Sent to Anthropic: temperature = 1.0
```

### Max Tokens Requirement

| OpenAI | Anthropic | Gateway Behavior |
|--------|-----------|------------------|
| Optional (default 4096) | **Required** | Adds default 4096 if missing |

### Role Name Mapping

| OpenAI | Anthropic | Notes |
|--------|-----------|-------|
| `"system"` | Extracted to `system` field | ✅ Clean conversion |
| `"user"` | `"user"` | ✅ Identical |
| `"assistant"` | `"assistant"` | ✅ Identical |
| `"tool"` | `"user"` with tool_result block | ✅ Converted |

### Content Block Conversion

OpenAI's flexible content format converts cleanly:

```json
// OpenAI
{
  "content": [
    {"type": "text", "text": "Hello"},
    {"type": "image_url", "image_url": {...}}
  ]
}

// Anthropic
{
  "content": [
    {"type": "text", "text": "Hello"},
    {"type": "image", "source": {...}}
  ]
}
```

**Trade-offs:**
- ✅ Multimodal content preserved
- ⚠️ HTTP image URLs fetched and converted to base64 (latency impact)
- ✅ Detail level preserved in metadata

---

## OpenAI → Gemini

### Parameters Dropped (With Warnings)

| Parameter | OpenAI Support | Gemini Equivalent | Gateway Behavior |
|-----------|----------------|-------------------|------------------|
| `seed` | Deterministic sampling | ❌ None | Dropped, warning header |
| `logprobs` | Token probabilities | ❌ None | Dropped, warning header |
| `top_logprobs` | Top-k token probs | ❌ None | Dropped, warning header |
| `logit_bias` | Token probability bias | ❌ None | Dropped, warning header |
| `service_tier` | Priority routing | ❌ None | Dropped, warning header |
| `presence_penalty` | Repetition reduction | ❌ None | Dropped, warning header |
| `frequency_penalty` | Frequency reduction | ❌ None | Dropped, warning header |
| `n` (if > 1) | Multiple completions | ⚠️ `candidateCount` | **Future support planned** |

### JSON Mode Implementation

| OpenAI | Gemini | Gateway Approach |
|--------|--------|------------------|
| `response_format: {"type": "json_object"}` | ✅ `response_mime_type` | ✅ Native conversion |
| `response_format: {"type": "json_schema"}` | ✅ `response_schema` | ✅ Native conversion |

**Conversion:**

```json
// OpenAI
{
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "colors",
      "schema": {...}
    }
  }
}

// Gemini
{
  "generationConfig": {
    "responseMimeType": "application/json",
    "responseSchema": {...}
  }
}
```

**Trade-offs:**
- ✅ Full schema enforcement
- ✅ Strict mode supported
- ✅ No workarounds needed

### Role Name Mapping

| OpenAI | Gemini | Notes |
|--------|--------|-------|
| `"system"` | Extracted to `systemInstruction` | ✅ Clean conversion |
| `"user"` | `"user"` | ✅ Identical |
| `"assistant"` | `"model"` | ✅ Mapped |
| `"tool"` | `"user"` with function response | ✅ Converted |

### Content Block Conversion

| OpenAI | Gemini | Gateway Approach |
|--------|--------|------------------|
| `"text"` content block | `Part` with `text` | ✅ Direct mapping |
| `"image_url"` content block | `Part` with `inlineData` | ✅ Converted (base64) |
| `"tool_use"` content block | `Part` with `functionCall` | ✅ Converted |
| `"tool_result"` content block | `Part` with `functionResponse` | ✅ Converted |

**Trade-offs:**
- ✅ All content types supported
- ⚠️ HTTP URLs fetched and converted to base64
- ✅ Multi-part messages preserved

---

## Anthropic → OpenAI

### Response Content Blocks

Anthropic responses can have multiple content blocks. The gateway flattens them:

| Anthropic | OpenAI | Gateway Behavior |
|-----------|--------|------------------|
| Multiple `"text"` blocks | Single `content` string | ✅ Concatenated |
| `"tool_use"` blocks | `tool_calls` array | ✅ Converted |
| `"thinking"` blocks (extended thinking) | Dropped | ⚠️ Lost in conversion |

**Example:**

```json
// Anthropic response
{
  "content": [
    {"type": "thinking", "thinking": "Let me analyze..."},
    {"type": "text", "text": "Based on analysis..."},
    {"type": "tool_use", "id": "...", "name": "...", "input": {...}}
  ]
}

// Converted to OpenAI
{
  "choices": [{
    "message": {
      "content": "Based on analysis...",  // Thinking block lost!
      "tool_calls": [...]
    }
  }]
}
```

**Trade-offs:**
- ✅ Text and tool calls preserved
- ⚠️ Image blocks in responses (rare) dropped
- ❌ Thinking blocks lost (no OpenAI equivalent)

### Token Usage with Caching

Anthropic returns cache metrics that don't exist in OpenAI's format:

| Anthropic | OpenAI | Gateway Behavior |
|-----------|--------|------------------|
| `input_tokens` | `prompt_tokens` | ✅ Mapped |
| `output_tokens` | `completion_tokens` | ✅ Mapped |
| `cache_creation_input_tokens` | ❌ None | ⚠️ **Dropped** |
| `cache_read_input_tokens` | ❌ None | ⚠️ **Dropped** |

**Workaround**: Access the raw Anthropic response to get cache metrics (not available through OpenAI-compatible interface).

### Stop Reason Mapping

| Anthropic | OpenAI | Notes |
|-----------|--------|-------|
| `"end_turn"` | `"stop"` | ✅ Mapped |
| `"max_tokens"` | `"length"` | ✅ Mapped |
| `"stop_sequence"` | `"stop"` | ✅ Mapped |
| `"tool_use"` | `"tool_calls"` | ✅ Mapped |

---

## Gemini → OpenAI

### Safety Ratings

Gemini returns safety ratings that don't exist in OpenAI's format:

```json
// Gemini response
{
  "candidates": [{
    "safetyRatings": [
      {"category": "HARM_CATEGORY_HARASSMENT", "probability": "LOW"},
      {"category": "HARM_CATEGORY_HATE_SPEECH", "probability": "NEGLIGIBLE"}
    ]
  }]
}

// Converted to OpenAI (safety ratings dropped)
{
  "choices": [{
    "message": {...}
  }]
}
```

**Trade-off**: Safety information is lost in conversion.

### Finish Reason Mapping

| Gemini | OpenAI | Notes |
|--------|--------|-------|
| `"STOP"` | `"stop"` | ✅ Mapped |
| `"MAX_TOKENS"` | `"length"` | ✅ Mapped |
| `"SAFETY"` | `"content_filter"` | ✅ Mapped |
| `"RECITATION"` | `"content_filter"` | ✅ Mapped (approximation) |
| `"OTHER"` | `"stop"` | ⚠️ Best effort mapping |

### Citation Metadata

Gemini can return citation metadata (grounding information):

```json
// Gemini
{
  "citationMetadata": {
    "citations": [...]
  }
}
```

**Trade-off**: Citation metadata is **dropped** (no OpenAI equivalent).

### Response Candidates

Gemini can return multiple candidates. The gateway uses only the first:

```json
// Gemini
{
  "candidates": [
    {"content": "Response A", ...},
    {"content": "Response B", ...}
  ]
}

// Converted to OpenAI (only first candidate)
{
  "choices": [
    {"message": {"content": "Response A"}}
  ]
}
```

**Trade-off**: Additional candidates are **dropped**.

---

## Best Practices

### 1. Choose the Right Provider

Match features to providers:

| Use Case | Recommended Provider | Reason |
|----------|---------------------|--------|
| **Strict JSON schema** | OpenAI or Gemini | Native support, guaranteed compliance |
| **Cost optimization** | Anthropic | Prompt caching (90% savings) |
| **Safety filtering** | Gemini | Built-in safety ratings |
| **Deterministic outputs** | OpenAI | `seed` parameter support |
| **Extended thinking** | Anthropic | Thinking blocks (native) |
| **Token probabilities** | OpenAI | `logprobs` support |

### 2. Design for the Lowest Common Denominator

If your app must work across all providers:

```typescript
// ✅ Good: Works everywhere
const request = {
  model: dynamicModel, // Could be any provider
  messages: [{role: "user", content: "Hello"}],
  max_tokens: 1000,
  temperature: 0.7  // Safe range: 0.0 - 1.0
};

// ❌ Bad: Only works with OpenAI
const request = {
  model: dynamicModel,
  messages: [{role: "user", content: "Hello"}],
  seed: 12345,           // Dropped by Anthropic/Gemini
  logprobs: true,        // Dropped by Anthropic/Gemini
  temperature: 1.5       // Clipped by Anthropic
};
```

### 3. Handle Conversion Warnings

Always check `X-LLM-Gateway-Warnings` header:

```typescript
const response = await fetch(gatewayUrl, {
  method: 'POST',
  headers: {...},
  body: JSON.stringify(request)
});

const warnings = response.headers.get('x-llm-gateway-warnings');
if (warnings) {
  const warningList = JSON.parse(warnings);
  warningList.forEach(w => {
    if (w.level === 'warning') {
      logger.warn(`Feature approximation: ${w.message}`);
    }
  });
}
```

### 4. Test Across Providers

If you support multiple providers, test with each:

```typescript
const providers = ['gpt-4', 'claude-3-5-sonnet', 'gemini-1.5-pro'];

for (const model of providers) {
  const response = await makeRequest({...request, model});
  validateResponse(response);
  checkWarnings(response.headers);
}
```

### 5. Use Provider-Specific Features Conditionally

```typescript
const request = {
  model: userSelectedModel,
  messages: [...],
  max_tokens: 1000
};

// Add provider-specific features only when needed
if (userSelectedModel.startsWith('claude')) {
  // Anthropic-specific
  if (largeSystemPrompt) {
    // Auto-caching will be applied by gateway
  }
  if (needsThinking) {
    request.thinking = {type: 'enabled', budget_tokens: 10000};
  }
}

if (userSelectedModel.startsWith('gemini')) {
  // Gemini-specific
  request.safety_settings = safetyConfig;
}

if (userSelectedModel.startsWith('gpt')) {
  // OpenAI-specific
  request.seed = deterministicSeed;
  request.logprobs = true;
}
```

### 6. Document Provider Differences

In your API documentation, clearly state:

```markdown
## Model Support

| Feature | GPT-4 | Claude | Gemini |
|---------|:-----:|:------:|:------:|
| Deterministic outputs (`seed`) | ✅ | ❌ | ❌ |
| Strict JSON schema | ✅ | ⚠️ | ✅ |
| Prompt caching | ❌ | ✅ | ❌ |
| Extended thinking | ❌ | ✅ | ❌ |

⚠️ = Supported via workaround (may not guarantee strict compliance)
```

### 7. Monitor Conversion Impact

Track which features generate warnings:

```promql
# Prometheus query
sum by (warning_type) (llm_conversion_warnings_total)
```

If many requests trigger warnings, consider:
- Switching to a provider with native support
- Simplifying requests to avoid unsupported features
- Implementing provider-specific code paths

---

## Provider Feature Comparison

### Core Features

| Feature | OpenAI | Anthropic | Gemini |
|---------|:------:|:---------:|:------:|
| **Text completion** | ✅ | ✅ | ✅ |
| **Streaming** | ✅ | ✅ | ✅ |
| **Vision** | ✅ | ✅ | ✅ |
| **Tool calling** | ✅ | ✅ | ✅ |
| **System prompts** | ✅ | ✅ | ✅ |

### Advanced Features

| Feature | OpenAI | Anthropic | Gemini |
|---------|:------:|:---------:|:------:|
| **Prompt caching** | ❌ | ✅ | ❌ |
| **Extended thinking** | ❌ | ✅ | ❌ |
| **JSON schema (strict)** | ✅ | ❌ | ✅ |
| **Safety ratings** | ❌ | ❌ | ✅ |
| **Deterministic sampling** | ✅ (`seed`) | ❌ | ❌ |
| **Token probabilities** | ✅ (`logprobs`) | ❌ | ❌ |
| **Multiple completions** | ✅ (`n` param) | ❌ | ⚠️ (`candidateCount`) |

### Parameter Support

| Parameter | OpenAI | Anthropic | Gemini |
|-----------|:------:|:---------:|:------:|
| `temperature` | 0.0 - 2.0 | 0.0 - 1.0 | 0.0 - 2.0 |
| `max_tokens` | Optional | **Required** | Optional |
| `top_p` | ✅ | ✅ | ✅ |
| `top_k` | ❌ | ✅ | ✅ |
| `stop` | ✅ | ✅ | ✅ |
| `presence_penalty` | ✅ | ❌ | ❌ |
| `frequency_penalty` | ✅ | ❌ | ❌ |

---

## Summary

### What Works Everywhere

These features convert cleanly across all providers:

✅ Basic text completion
✅ Streaming
✅ Vision (images)
✅ Tool calling (functions)
✅ System prompts
✅ Temperature (0.0 - 1.0 range)
✅ Top-p sampling
✅ Stop sequences
✅ Max tokens

### What Has Limitations

These features work but with trade-offs:

⚠️ **JSON mode** - Native on OpenAI/Gemini, workaround on Anthropic
⚠️ **JSON schema** - Native on OpenAI/Gemini, best-effort on Anthropic
⚠️ **Image URLs** - Automatically fetched and converted (latency impact)
⚠️ **Temperature > 1.0** - Clipped to 1.0 for Anthropic

### What Doesn't Work

These features are provider-specific only:

❌ **Prompt caching** - Anthropic only
❌ **Extended thinking** - Anthropic only
❌ **Deterministic sampling (`seed`)** - OpenAI only
❌ **Token probabilities (`logprobs`)** - OpenAI only
❌ **Safety ratings** - Gemini only
❌ **Penalties (`presence_penalty`, `frequency_penalty`)** - OpenAI only

---

For more information:
- [FEATURES.md](FEATURES.md) - Comprehensive feature documentation
- [README.md](../README.md) - Quick start guide
- [PHASES_COMPLETE.md](../PHASES_COMPLETE.md) - Implementation status
