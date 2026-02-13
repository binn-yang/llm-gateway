# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

LLM Gateway 是一个高性能 Rust 代理服务,为多个 LLM 提供商提供统一 API:
- **OpenAI 兼容 API** (`/v1/chat/completions`) - 通过协议转换支持所有提供商(前缀路由)
- **原生 Anthropic API** (`/v1/messages`) - Claude 模型的直接透传
- **路径路由端点** - Azure、Bedrock、Responses API、自定义 Provider 直连(绕过 ModelRouter)

核心特性:基于 trait 的可插拔 Provider 架构、基于优先级的粘性会话负载均衡、自动故障转移、SQLite 可观测性系统、Web Dashboard、完整的 token 跟踪(包括 Anthropic 提示缓存指标)。

**版本**: 0.5.0
**技术栈**: Rust + Axum + Tokio + SQLite (后端) + Vue 3 + TypeScript + Chart.js (前端)

## 最新更新 (v0.5.0)

### Provider 故障切换优化
- ✅ **智能错误分类**: 401/403/429/503 特殊处理,不同错误类型采取不同策略
- ✅ **熔断器模式**: 3 次失败触发熔断,半开状态测试恢复,2 次成功关闭
- ✅ **自适应恢复**: 指数退避(60s → 600s) + Jitter,替代固定 60 秒
- ✅ **自动重试**: 429 延迟重试,503 立即重试,实例故障自动切换
- ✅ **健康状态可视化**: stats 命令显示实时健康状态,failover_events 表记录事件
- ✅ **零配置**: 硬编码合理默认值,无需修改配置文件

## 基本命令

### 构建和运行
```bash
cargo build --release               # 生产构建
cargo run --release                 # 运行服务
cargo test                          # 运行测试
./target/release/llm-gateway test  # 测试配置
./target/release/llm-gateway start # 启动服务
```

### OAuth 认证 (v0.5.0)
```bash
# Anthropic OAuth 登录(统一手动粘贴流程)
./target/release/llm-gateway oauth login anthropic
# 1. 复制显示的授权 URL 到浏览器打开
# 2. 授权后手动复制完整的回调 URL
# 3. 粘贴回调 URL 到 CLI 提示符

# Gemini OAuth 登录 (gemini-cli / antigravity)
./target/release/llm-gateway oauth login gemini-cli
./target/release/llm-gateway oauth login antigravity

# 查看状态
./target/release/llm-gateway oauth status anthropic
./target/release/llm-gateway oauth status gemini-cli

# 刷新/登出
./target/release/llm-gateway oauth refresh anthropic
./target/release/llm-gateway oauth logout anthropic
```

### 配置管理
```bash
./target/release/llm-gateway config validate  # 验证配置
./target/release/llm-gateway config show      # 显示配置(脱敏)
```

### 重要文件
- `config.toml` - **禁止提交真实 API 密钥** (已在 .gitignore 中)
- `config.toml.example` - 配置模板

## 架构概述

### 请求流程

**前缀路由** (`/v1/chat/completions`) — ModelRouter 根据模型名前缀选择 provider:
```
客户端请求 → Auth中间件 → ModelRouter(前缀匹配) → ProviderRegistry(查找)
→ LoadBalancer(粘性会话) → Retry层 → LlmProvider.send_request() → 上游API
```

**原生 Anthropic API** (`/v1/messages`) — 直接路由到 Anthropic provider:
```
客户端请求 → Auth中间件 → ProviderRegistry("anthropic")
→ LoadBalancer(粘性会话) → Retry层 → AnthropicProvider.send_request()
```

**路径路由** (`/azure/*`, `/bedrock/*`, `/v1/responses`, `/custom/:id/*`) — URL 直接确定 provider:
```
客户端请求 → Auth中间件 → ProviderRegistry(从URL提取provider名)
→ LoadBalancer(粘性会话) → Retry层 → LlmProvider.send_request() → 上游API
```

### Provider 架构 (Trait-based)

架构基于三个核心 trait/struct,新增 Provider 无需修改 match arm:

#### ProviderConfig trait (`src/provider_config.rs`)
统一的实例配置接口,替代旧的 `ProviderInstanceConfigEnum` 枚举:
```rust
pub trait ProviderConfig: Send + Sync + Debug + 'static {
    fn name(&self) -> &str;
    fn enabled(&self) -> bool;
    fn auth_mode(&self) -> &AuthMode;
    fn api_key(&self) -> Option<&str>;
    fn oauth_provider(&self) -> Option<&str>;
    fn base_url(&self) -> &str;
    fn timeout_seconds(&self) -> u64;
    fn priority(&self) -> u32;
    fn failure_timeout_seconds(&self) -> u64;
    fn weight(&self) -> u32;
    fn as_any(&self) -> &dyn Any;  // downcast 到具体配置类型
}
```

#### LlmProvider trait (`src/provider_trait.rs`)
统一的 Provider 发送接口,封装 URL 构造 + 认证 + 请求发送:
```rust
pub trait LlmProvider: Send + Sync + 'static {
    fn provider_type(&self) -> &str;
    fn native_protocol(&self) -> ProviderProtocol;  // OpenAI | Anthropic | Gemini
    async fn send_request(&self, client: &Client, config: &dyn ProviderConfig,
                          request: UpstreamRequest) -> Result<Response, AppError>;
    fn health_check_url(&self, config: &dyn ProviderConfig) -> String;
}
```

#### ProviderRegistry (`src/registry.rs`)
字符串键的注册中心,替代 `HashMap<Provider, Arc<LoadBalancer>>`:
```rust
pub struct RegisteredProvider {
    pub provider: Arc<dyn LlmProvider>,
    pub load_balancer: Arc<LoadBalancer>,
}
pub struct ProviderRegistry {
    providers: HashMap<String, RegisteredProvider>,  // "openai", "anthropic", "custom:deepseek", ...
}
```

#### 已实现的 Provider

| Provider | 类型 | 协议 | 认证方式 | 路径路由 |
|----------|------|------|----------|----------|
| `OpenAIProvider` | openai | OpenAI | Bearer | `/v1/chat/completions` (前缀路由) |
| `AnthropicProvider` | anthropic | Anthropic | x-api-key / OAuth | `/v1/messages` |
| `GeminiProvider` | gemini | Gemini | query param / OAuth | `/v1beta/models/*` |
| `AzureOpenAIProvider` | azure_openai | OpenAI | api-key header | `/azure/v1/chat/completions` |
| `BedrockProvider` | bedrock | Anthropic | AWS SigV4 (手动) | `/bedrock/v1/messages` |
| `OpenAIResponsesProvider` | openai (复用) | OpenAI | Bearer | `/v1/responses` |
| `CustomOpenAIProvider` | custom:{id} | OpenAI | Bearer + 自定义 headers | `/custom/:provider_id/v1/chat/completions` |

#### AppState
```rust
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub router: Arc<ModelRouter>,
    pub http_client: reqwest::Client,
    pub registry: Arc<ArcSwap<ProviderRegistry>>,  // 替代旧的 load_balancers
    pub request_logger: Option<Arc<RequestLogger>>,
    pub token_store: Option<Arc<TokenStore>>,
    pub oauth_manager: Option<Arc<OAuthManager>>,
}
```

### 核心组件

#### 1. 负载均衡与故障切换系统 (`src/load_balancer.rs`, `src/retry.rs`, `src/error.rs`)
- **粘性会话**: 每个 API 密钥绑定到特定实例 1 小时(最大化提供商侧 KV 缓存命中)
- **优先级选择**: 数字越小优先级越高,相同优先级随机选择
- **智能错误分类** (v0.5.0+):
  - 401/403 认证错误 → 标记实例故障(配置问题)
  - 429 Rate Limit → 延迟重试,自动切换实例
  - 503 Service Unavailable → 立即重试其他实例(瞬时过载)
  - 500/502/504 → 标记实例故障
  - 4xx 业务错误 → 直接返回,不触发故障转移
- **熔断器模式** (v0.5.0+):
  - 3 次失败(60秒窗口)触发熔断器打开
  - 半开状态测试恢复(健康检测通过后)
  - 2 次成功请求关闭熔断器
  - 状态机: Closed → Open → HalfOpen → Closed
- **自适应恢复**:
  - 指数退避: 60s → 120s → 240s → 480s → 600s (最大)
  - ±20% Jitter 防止惊群效应
  - 主动健康检测替代被动等待
- **自动重试**:
  - 最大重试 3 次防止无限循环
  - 429 延迟 retry_after 秒后重试
  - 瞬时错误立即重试不同实例
  - 实例故障自动切换到备份实例
- **可观测性**:
  - failover_events 表记录所有故障转移事件
  - stats 命令显示实时健康状态(✅ 健康 / 🟡 恢复中 / 🔴 不健康)
  - 非阻塞事件记录(tokio::spawn)

#### 2. 模型路由 (`src/router.rs`)
使用前缀匹配:
```toml
[routing.rules]
"gpt-" = "openai"
"claude-" = "anthropic"
"gemini-" = "gemini"
```

#### 3. 协议转换 (`src/converters/*`)
- OpenAI ↔ Anthropic ↔ Gemini 双向转换
- 处理系统消息格式差异、角色名称、max_tokens、temperature 范围等
- 支持流式响应转换

#### 4. OAuth 认证系统 (`src/oauth/*`)
支持两种认证模式:
- **Bearer**: 使用 API 密钥(默认)
- **OAuth**: 使用 OAuth token

**Anthropic OAuth 配置**(官方凭证):
```toml
[[oauth_providers]]
name = "anthropic"
client_id = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
auth_url = "https://claude.ai/oauth/authorize"
token_url = "https://console.anthropic.com/v1/oauth/token"
redirect_uri = "https://platform.claude.com/oauth/code/callback"
scopes = ["org:create_api_key", "user:profile", "user:inference", "user:sessions:claude_code"]

[[providers.anthropic]]
name = "anthropic-oauth"
auth_mode = "oauth"
oauth_provider = "anthropic"
# 无需 api_key
```

**Gemini OAuth 配置**(官方凭证):
```toml
# Gemini CLI OAuth Provider (gemini-cli 官方应用)
[[oauth_providers]]
name = "gemini-cli"
# 官方 Gemini CLI 公开客户端凭证
client_id = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com"
# 客户端密钥（公开 OAuth 客户端）
client_secret = "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"
# Google OAuth 授权端点
auth_url = "https://accounts.google.com/o/oauth2/v2/auth"
# Google OAuth Token 端点
token_url = "https://oauth2.googleapis.com/token"
# 回调地址（gemini-cli 官方回调地址）
redirect_uri = "https://codeassist.google.com/authcode"
# 必需权限
scopes = ["https://www.googleapis.com/auth/cloud-platform"]

# Antigravity OAuth Provider (Antigravity 应用)
[[oauth_providers]]
name = "antigravity"
# Antigravity 官方公开客户端凭证
client_id = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com"
# 客户端密钥（公开 OAuth 客户端）
client_secret = "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"
# Google OAuth 授权端点
auth_url = "https://accounts.google.com/o/oauth2/v2/auth"
# Google OAuth Token 端点
token_url = "https://oauth2.googleapis.com/token"
# 回调地址（授权后手动复制完整 URL 粘贴到 CLI）
redirect_uri = "http://localhost:45462"
# 完整权限列表
scopes = [
  "https://www.googleapis.com/auth/cloud-platform",
  "https://www.googleapis.com/auth/userinfo.email",
  "https://www.googleapis.com/auth/userinfo.profile",
  "https://www.googleapis.com/auth/cclog",
  "https://www.googleapis.com/auth/experimentsandconfigs"
]

[[providers.gemini]]
name = "gemini-oauth"
auth_mode = "oauth"
oauth_provider = "gemini-cli"
# 无需 api_key
```

**Token 管理**:
- 自动刷新:后台任务每 5 分钟检查,过期前 10 分钟刷新
- 按需刷新:每次请求前检查,过期前 1 分钟刷新
- 存储位置:`~/.llm-gateway/oauth_tokens.json`(AES-256-GCM 加密)

#### 5. 可观测性系统 (`src/observability/*`)
- **SQLite 数据库**:请求日志、token 使用、性能指标
- **JSONL 文件日志**:详细请求追踪(`logs/requests.YYYY-MM-DD`)
- **Body 日志**:请求/响应体记录,敏感数据自动脱敏
- **数据保留**:日志 7 天,指标 30 天,自动清理

#### 6. Token 跟踪 (`src/streaming.rs`)
跟踪 4 种 token 类型:
- `input_tokens`: 输入 token
- `output_tokens`: 输出 token
- `cache_creation_input_tokens`: 缓存创建 token(+25% 成本)
- `cache_read_input_tokens`: 缓存读取 token(-90% 成本)

从 `message_delta` 事件统一提取,兼容 Anthropic 官方 API 和 GLM 提供商。

#### 7. 配置管理系统 (`src/config_db.rs`, `src/handlers/config_api.rs`)
- **数据库驱动**:配置存储在 SQLite(`./data/config.db`)
- **热重载**:配置变更立即生效,无需重启服务
- **Web UI**:Vue 3 前端管理 API 密钥、路由规则、提供商实例
- **API 端点**:`/api/config/*`(CRUD 操作)

#### 8. 定价与成本计算系统 (`src/pricing/*`)
**核心组件**:
- **PricingService** (`service.rs`): 定价数据缓存服务,从数据库加载并缓存模型定价
- **CostCalculator** (`calculator.rs`): 成本计算器,支持 input/output/cache tokens 的成本计算
- **PricingUpdater** (`updater.rs`): 自动更新器,每小时从远程源同步定价数据
- **PricingLoader** (`loader.rs`): 定价数据加载器,支持远程 JSON 和数据库加载

**工作流程**:
1. 服务启动时同步加载定价数据到缓存(确保首次请求即可计算成本)
2. 后台任务每小时检查远程定价数据更新
3. RequestLogger 在记录请求时自动计算成本
4. 流式响应在 token 提取完成后重新计算成本(修复了初始成本为 0 的问题)

**成本计算**:
- Input tokens: `input_tokens × input_price / 1,000,000`
- Output tokens: `output_tokens × output_price / 1,000,000`
- Cache write: `cache_creation_tokens × cache_write_price / 1,000,000`
- Cache read: `cache_read_tokens × cache_read_price / 1,000,000`

**数据库表**:
- `model_prices`: 存储模型定价数据(input/output/cache 价格)
- `pricing_metadata`: 存储定价数据元信息(版本、更新时间、哈希)
- `requests`: 新增成本字段(input_cost, output_cost, cache_write_cost, cache_read_cost, total_cost)

### Handlers (`src/handlers/`)

**前缀路由端点** (通过 ModelRouter 选择 provider):
- `chat_completions.rs` - `POST /v1/chat/completions`(OpenAI 兼容,自动协议转换)
- `messages.rs` - `POST /v1/messages`(原生 Anthropic API)
- `models.rs` - `GET /v1/models`(模型列表)
- `gemini_native.rs` - `GET/POST /v1beta/models/*`(Gemini 原生 API)

**路径路由端点** (绕过 ModelRouter,URL 直接确定 provider):
- `azure.rs` - `POST /azure/v1/chat/completions`(Azure OpenAI 直连)
- `bedrock.rs` - `POST /bedrock/v1/messages`(AWS Bedrock 直连)
- `openai_responses.rs` - `POST /v1/responses`(OpenAI Responses API)
- `custom.rs` - `POST /custom/:provider_id/v1/chat/completions`(自定义 provider)

**公共函数**:
- `common.rs` - `resolve_oauth_token()` OAuth token 解析(handler 间共享)

**其他**:
- `config_api.rs` - `/api/config/*`(配置管理 CRUD + 热重载)
- `health.rs` - `/health`, `/ready`(健康检查)

**使用建议**:
- 多提供商支持/OpenAI 工具兼容 → 使用 `/v1/chat/completions`(前缀路由)
- Claude Code/官方 SDK/Anthropic 特性 → 使用 `/v1/messages`
- 指定 provider 直连 → 使用路径路由(`/azure/*`, `/bedrock/*`, `/custom/:id/*`)
- OpenAI Responses API → 使用 `/v1/responses`

### 前端 Dashboard (`frontend/`)

**技术栈**: Vue 3 + TypeScript + Vite + Chart.js + Tailwind CSS

**功能**:
- Token 使用图表(时序、按 API 密钥、按实例)
- 提供商健康监控
- 请求追踪时间线
- 成本计算器
- 配置管理 UI(API 密钥、路由规则、提供商实例)

**开发**:
```bash
cd frontend
npm install
npm run dev    # http://localhost:3000
npm run build
```

## 配置模式

### 多实例配置
```toml
# 主实例(优先)
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1
enabled = true
api_key = "sk-ant-..."
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
failure_timeout_seconds = 60

# 备份实例(仅在主实例失败时使用)
[[providers.anthropic]]
name = "anthropic-backup"
priority = 2
# ... 其他配置 ...
```

### Azure OpenAI 配置
```toml
[[providers.azure_openai]]
name = "azure-primary"
enabled = true
api_key = "your-azure-api-key"
resource_name = "my-openai-resource"
api_version = "2024-02-01"
# deployment_name = "gpt-4"  # 可选,默认用模型名
timeout_seconds = 300
priority = 1

# 模型到 deployment 映射(可选)
[providers.azure_openai.model_deployments]
"gpt-4" = "gpt-4-deployment"
"gpt-4o" = "gpt-4o-deployment"
```

### AWS Bedrock 配置
```toml
[[providers.bedrock]]
name = "bedrock-primary"
enabled = true
region = "us-east-1"
access_key_id = "AKIA..."
secret_access_key = "..."
# session_token = "..."  # 可选,用于临时凭证
timeout_seconds = 300
priority = 1

# 模型 ID 映射(可选,友好名 → Bedrock model ID)
[providers.bedrock.model_id_mapping]
"claude-3-5-sonnet" = "anthropic.claude-3-5-sonnet-20241022-v2:0"
"claude-3-haiku" = "anthropic.claude-3-haiku-20240307-v1:0"
```

### 自定义 OpenAI 兼容 Provider 配置
```toml
[[providers.custom]]
name = "deepseek-primary"
enabled = true
provider_id = "deepseek"          # registry 中注册为 "custom:deepseek"
api_key = "sk-..."
base_url = "https://api.deepseek.com/v1"
timeout_seconds = 300
priority = 1

# 自定义请求 headers(可选)
[providers.custom.custom_headers]
"X-Custom-Header" = "value"
```

### 路由配置
```toml
[routing]
default_provider = "openai"

[routing.rules]
"gpt-" = "openai"
"claude-" = "anthropic"
"deepseek-" = "custom:deepseek"    # 自定义 provider 前缀路由

[routing.discovery]
enabled = true
cache_ttl_seconds = 3600
providers_with_listing = ["openai"]
```

### 可观测性配置
```toml
[observability]
enabled = true
database_path = "./data/observability.db"

[observability.performance]
batch_size = 100              # 每批事件数
flush_interval_ms = 100       # 最大刷新间隔
max_buffer_size = 10000       # 环形缓冲区大小

[observability.retention]
logs_days = 7                 # 日志保留天数
cleanup_hour = 3              # 每日清理时间(3:00 AM)

[observability.body_logging]
enabled = true
max_body_size = 102400        # 100KB

[[observability.body_logging.redact_patterns]]
pattern = "sk-[a-zA-Z0-9]{48}"
replacement = "sk-***REDACTED***"
```

## 测试模式

### 配置助手
```rust
use crate::config::{ProviderInstanceConfig, AnthropicInstanceConfig, ProvidersConfig};

fn create_test_config() -> Config {
    Config {
        providers: ProvidersConfig {
            openai: vec![ProviderInstanceConfig { /* ... */ }],
            anthropic: vec![AnthropicInstanceConfig { /* ... */ }],
            gemini: vec![],
            azure_openai: vec![],
            bedrock: vec![],
            custom: vec![],
        },
        // ...
    }
}
```

**重要**: ProvidersConfig 所有字段都必须提供(即使是空 vec)。

## 常见修改模式

### 添加新提供商
新的 trait-based 架构下,添加 provider 只需 3 步(无需修改 match arm):

1. **配置** (`src/config.rs`): 定义 `XxxInstanceConfig` struct,添加到 `ProvidersConfig`
2. **ProviderConfig impl** (`src/provider_config.rs`): 为新 config 实现 `ProviderConfig` trait
3. **LlmProvider impl** (`src/providers/xxx.rs`): 创建 Provider struct,实现 `LlmProvider` trait
4. **注册** (`src/server.rs`): 在 `create_provider_registry()` 中注册
5. **Handler** (`src/handlers/xxx.rs`): 如需路径路由,添加专用 handler + 路由

如需前缀路由(通过 `/v1/chat/completions`),还需:
6. 在 `routing.rules` 中添加前缀 → provider 映射
7. 如不兼容 OpenAI 协议,在 `src/converters/` 中添加转换器

### 修改健康检测
编辑 `src/retry.rs` 中的 `is_instance_failure()`:
- 返回 `true` = 标记实例不健康,触发故障转移
- 返回 `false` = 作为业务错误处理,不触发故障转移
- 当前触发条件:5xx 状态、连接错误、超时

### 添加实例级指标
在 `src/retry.rs::execute_with_session()` 中记录指标:
- 成功:`record_instance_request(provider, instance, "success")`
- 实例失败:`record_instance_request(provider, instance, "failure")`
- 业务错误:`record_instance_request(provider, instance, "business_error")`

## 安全考虑

- **API 密钥**:永远不要提交带有真实密钥的 `config.toml`
- **模型名称验证**:路由器验证模型名称(仅限字母数字 + `-._/`,1-256 字符)
- **请求大小限制**:10MB 最大 body 大小
- **认证**:通过中间件的 Bearer token 认证

## 性能要点

- **锁策略**:会话使用 DashMap(段锁),健康状态使用 RwLock(读重)
- **零分配**:使用 Arc 共享配置/实例
- **会话 TTL**:1 小时不活动超时,请求时自动刷新
- **后台任务**:会话清理(5 分钟),健康恢复(10 秒)

## 部署

### Docker
```bash
docker build -t llm-gateway .
docker run -p 8080:8080 -v $(pwd)/config.toml:/app/config.toml llm-gateway
```

### Nginx 水平扩展
使用 `Authorization` 头一致性哈希实现两层粘性:
```nginx
upstream llm_gateway_cluster {
    hash $http_authorization consistent;
    server gateway-1:8080;
    server gateway-2:8080;
}
```

效果:Layer 1(Nginx): API 密钥 → 特定网关实例,Layer 2(Gateway): API 密钥 → 特定提供商实例,结果:最大化 KV 缓存命中,无需共享状态。

## 开发最佳实践

### 数据模型设计原则

#### 1. 避免对外部数据使用过严格的类型
**问题**:对外部客户端数据使用带必填字段的严格 Rust 类型会导致反序列化失败。

**解决方案**:对只需透传的字段使用 `serde_json::Value`:
```rust
pub struct ContentBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,  // 灵活接受任何格式
}
```

**何时使用 `Value` vs 强类型**:
- 使用 `Value`:透传字段、上游 API 验证的字段、多格式字段、外部客户端数据
- 使用强类型:网关需读/改的字段、有业务逻辑的字段、内部数据结构、可控的配置文件

#### 2. 注意 `#[serde(untagged)]` 枚举陷阱
使用 `#[serde(untagged)]` 时,serde 按顺序尝试所有变体。如果任何字段在所有变体中都失败,整个反序列化失败。

**缓解措施**:
- 使内部类型灵活(对透传字段使用 `Value`)
- 添加带回退行为的自定义反序列化函数
- 对可选字段记录警告而非错误

#### 3. 网关职责:转发,而非验证
**核心原则**:网关的工作是路由和转发,而不是验证上游 API 契约。

```rust
// ✅ 正确:原样转发,让上游验证
let request: MessagesRequest = serde_json::from_value(raw_request)?;
providers::anthropic::create_message(&client, config, request).await
```

**原因**:
- 官方客户端(如 Claude Code CLI)发送正确格式
- 网关移除字段会丢失信息
- 上游 API 会在格式错误时返回正确的错误
- 网关不应质疑官方客户端

#### 4. 官方客户端兼容性至关重要
**始终记住**:像 Claude Code CLI 这样的客户端是同一公司(Anthropic)的官方工具。如果网关无法处理它们的请求,**是网关错了,不是客户端**。

**添加新模型时的检查清单**:
- [ ] 模型能接受官方客户端可能发送的所有变体吗?
- [ ] 必填字段真的是规范要求的,还是只是为了方便?
- [ ] 严格验证会破坏与未来客户端版本的兼容性吗?
- [ ] 是否有参考实现(如 claude-relay-service)可供比较?

### 错误处理模式

区分网关错误和上游错误(`src/retry.rs`):
```rust
pub fn classify_error(error: &AppError) -> FailureType {
    match error {
        // 401/403 认证错误 - 配置问题,标记实例故障
        AppError::UpstreamError { status, .. } if matches!(status.as_u16(), 401 | 403) => {
            FailureType::InstanceFailure
        }

        // 429 Rate Limit - 延迟重试
        AppError::RateLimitError { retry_after, .. } => FailureType::RateLimit {
            retry_after_secs: retry_after.unwrap_or(2),
        },

        // 503 Service Unavailable - 瞬时过载,立即重试
        AppError::UpstreamError { status, .. } if status.as_u16() == 503 => {
            FailureType::Transient
        }

        // 500/502/504 - 实例故障
        AppError::UpstreamError { status, .. } if matches!(status.as_u16(), 500 | 502 | 504) => {
            FailureType::InstanceFailure
        }

        // 业务错误 - 不触发故障转移
        _ => FailureType::BusinessError,
    }
}
```

**故障转移策略**:
- ✅ 实例故障(5xx/连接/超时) → 标记不健康 + 自动切换
- ✅ 认证错误(401/403) → 标记不健康(配置问题)
- ✅ Rate Limit(429) → 延迟 retry_after 秒 + 切换实例
- ✅ 瞬时错误(503) → 立即重试不同实例(不标记不健康)
- ✅ 业务错误(4xx) → 直接返回给客户端

**熔断器配置**(硬编码默认值):
```rust
const FAILURE_THRESHOLD: u32 = 3;           // 3 次失败触发熔断
const FAILURE_WINDOW_SECS: u64 = 60;        // 60 秒窗口
const SUCCESS_THRESHOLD: u32 = 2;           // 2 次成功关闭熔断器
const INITIAL_BACKOFF_SECS: u64 = 60;       // 初始退避 60 秒
const MAX_BACKOFF_SECS: u64 = 600;          // 最大退避 10 分钟
const BACKOFF_MULTIPLIER: f64 = 2.0;        // 指数倍增
const JITTER_RATIO: f64 = 0.2;              // ±20% 抖动
```

### 常见错误

1. ❌ 添加上游 API 已做的验证
2. ❌ 移除不理解的字段
3. ❌ 为方便而将字段设为必填
4. ❌ 不使用官方客户端测试
5. ❌ 假设你的类型定义是"正确的"

### 版本兼容性

当上游 API 添加新字段或格式时:
- ✅ 网关应无需代码更改即可工作(如对透传字段使用 `Value`)
- ✅ 客户端可立即采用新特性(网关不阻塞)
- ❌ 不要求每次上游 API 更改都更新网关

这就是为什么对只透传的字段优先使用 `serde_json::Value`。

## 故障排查

### 成本计算为 0 的问题

**症状**: 数据库中的请求记录显示 `total_cost = 0.0`,即使有 token 使用数据。

**根本原因**: 流式响应的时序竞争问题
1. 流式请求开始时,`RequestEvent` 的所有 token 字段都是 0
2. `log_request()` 立即被调用,基于 0 token 计算成本 → $0.00
3. Token 数据稍后从 `message_delta` 事件提取并更新到数据库
4. 但成本不会重新计算,仍然保持 $0.00

**解决方案** (已在 v0.5.0 修复):
- `update_tokens()` 方法现在会重新计算成本
- 使用真实的 token 数据进行成本计算
- 同时更新 token 和成本字段到数据库

**验证方法**:
```bash
# 发送流式请求
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer YOUR_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-haiku-4-5-20251001","messages":[{"role":"user","content":"hi"}],"max_tokens":50,"stream":true}'

# 检查成本数据
sqlite3 data/observability.db "SELECT model, input_tokens, output_tokens, total_cost FROM requests ORDER BY timestamp DESC LIMIT 1;"
```

**预期结果**: `total_cost` 应该大于 0,且与 token 数量成正比。
