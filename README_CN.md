# LLM Gateway

一个用 Rust 编写的高性能 LLM 代理网关,为多个 LLM 提供商(OpenAI、Anthropic Claude、Google Gemini)提供多种 API 格式:
- **统一的 OpenAI 兼容 API** (`/v1/chat/completions`) - 通过自动协议转换支持所有提供商
- **原生 Anthropic Messages API** (`/v1/messages`) - 直接透传 Claude 模型,无需协议转换
- **基于 SQLite 的可观测性** - 完整的请求日志记录,包含 token 跟踪和性能指标
- **Web 仪表板** - 使用 Vue 3 构建的实时监控和分析界面

## 功能特性

- **多种 API 格式**:
  - 统一的 OpenAI 兼容 API (`/v1/chat/completions`),自动进行协议转换
  - 原生 Anthropic Messages API (`/v1/messages`),直接访问 Claude
- **协议转换**: 自动在 OpenAI、Anthropic 和 Gemini 格式之间进行请求/响应转换
- **智能路由**: 基于前缀的模型路由到相应提供商
- **多实例负载均衡**: 每个提供商支持多个后端实例,基于优先级选择
- **粘性会话**: API 密钥级别的会话亲和性,最大化提供商端的 KV 缓存命中
- **自动故障转移**: 单次请求失败触发即时故障转移并自动恢复
- **基于 SQLite 的可观测性**:
  - 完整的请求日志记录,包含 token 使用跟踪
  - Anthropic 提示缓存指标(缓存创建/读取 tokens)
  - 自动数据保留策略(7-30 天)
  - 非阻塞异步批量写入
- **Web 仪表板**(新):
  - 实时 token 使用图表和分析
  - 提供商实例健康监控
  - 按 API 密钥的成本估算
  - 请求追踪可视化
  - **配置管理界面** - API 密钥、路由规则和提供商实例的 CRUD 操作
- **灵活的配置**:
  - 基于数据库的配置,支持热重载(无需重启服务器)
  - Web UI 管理 API 密钥、路由规则和提供商实例
  - 支持 TOML 文件,用于向后兼容和初始设置
  - 双重认证:网关 API 密钥(SHA256 哈希)+ 提供商 API 密钥(加密存储)
- **基于 SQLite 的指标**: 统一的可观测性,按请求粒度和自动保留
- **流式支持**: 完整的 SSE 支持,实时协议转换
- **云原生**: Docker 就绪,健康检查,结构化 JSON 日志
- **横向扩展**: 与 Nginx 兼容,支持多机部署

## 架构

网关提供两种 API 格式:

```
┌─────────────────────────────────────────────────────────────────┐
│  选项 1: OpenAI 兼容 API(所有提供商)                              │
│  ┌─────────────┐                                                │
│  │   Cursor    │                                                │
│  │  Continue   │  → /v1/chat/completions → 网关 →               │
│  │   等工具    │                          自动路由到:            │
│  └─────────────┘                          ├─ OpenAI(直连)      │
│                                            ├─ Anthropic(转换)   │
│                                            └─ Gemini(转换)      │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  选项 2: 原生 Anthropic API(仅 Claude)                           │
│  ┌─────────────┐                                                │
│  │ Claude Code │  → /v1/messages → 网关 → Anthropic             │
│  │  Anthropic  │                   (原生格式,无转换)             │
│  │    SDK      │                                                │
│  └─────────────┘                                                │
└─────────────────────────────────────────────────────────────────┘
```

## 负载均衡与高可用

### 多提供商实例架构

每种提供商类型(OpenAI、Anthropic、Gemini)都可以拥有**多个后端实例**,用于负载均衡和自动故障转移:

```
┌──────────────────────────────────────────────────────────┐
│  客户端请求 (API Key = "sk-user-alice")                    │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────┐
│  网关: LoadBalancer (基于优先级的粘性会话)                 │
│  ┌────────────────────────────────────────────────────┐  │
│  │  SessionMap (API Key → 实例绑定)                   │  │
│  │  - "sk-user-alice" → "anthropic-primary"           │  │
│  │  - 会话 TTL: 1 小时(请求时自动刷新)                 │  │
│  └────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  HealthState (实例 → 健康状态)                     │  │
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
│  主实例      │         │  备份实例    │
│  priority=1  │         │  priority=2  │
└──────────────┘         └──────────────┘
```

### 粘性会话策略

**为什么使用粘性会话?**
- **最大化 KV 缓存命中**: 同一用户 → 同一实例 → 提供商可以重用对话上下文
- **最小化锁竞争**: 使用 DashMap 的分段锁 + RwLock 用于读密集型健康检查
- **可预测的性能**: 不会出现破坏缓存局部性的随机负载分配

**工作原理:**
1. **首次请求**: 用户发起初始请求 → LoadBalancer 按优先级选择实例
2. **会话创建**: API 密钥绑定到选定实例 1 小时
3. **后续请求**: 同一 API 密钥始终路由到同一实例(直到失败或超时)
4. **会话过期**: 1 小时无活动后,会话过期 → 下次请求按优先级重新选择

### 基于优先级的选择

实例配置有**优先级**值(数字越小 = 优先级越高):

```toml
# 主实例(健康时始终优先)
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1                    # 最高优先级

# 备份实例(仅当主实例失败时使用)
[[providers.anthropic]]
name = "anthropic-backup"
priority = 2                    # 较低优先级

# 另一个备份(相同优先级 = 随机选择)
[[providers.anthropic]]
name = "anthropic-backup-2"
priority = 2                    # 相同优先级 → 在这两个之间随机选择
```

**选择算法:**
1. 过滤: 仅健康且已启用的实例
2. 在健康实例中找到最小优先级值
3. 在具有该优先级的实例中随机选择
4. 将 API 密钥绑定到选定实例(粘性会话)

### 自动故障转移与恢复

#### 健康检测标准

以下类型的**单次请求失败**会将实例标记为**不健康**:

| 失败类型 | 示例 | 操作 |
|--------------|----------|--------|
| **5xx 服务器错误** | 500, 502, 503, 504 | 标记为不健康 |
| **连接失败** | TCP 超时、连接拒绝、DNS 失败 | 标记为不健康 |
| **请求超时** | 超过 `timeout_seconds` | 标记为不健康 |
| **4xx 客户端错误** | 401, 403, 429 | **无操作**(非实例故障) |
| **业务错误** | 无效的 API 密钥、速率限制 | **无操作** |

#### 自动恢复机制

**基于时间的被动恢复**(无主动健康探测):

```
时间线示例:

T+0s:    请求在主实例上成功
         ✓ 会话: sk-user-alice → primary

T+30s:   请求在主实例上失败(502 Bad Gateway)
         ✗ 主实例标记为不健康
         ✓ 会话不变(此请求失败)

T+35s:   下一个请求检测到主实例不健康
         → 删除会话
         → 选择备份实例(priority=2)
         ✓ 新会话: sk-user-alice → backup

T+90s:   主实例自动恢复(60秒超时已过)
         ✓ 主实例再次标记为健康
         ✓ 用户仍在备份上(会话活跃)

T+3635s: 会话过期(自上次请求起 1 小时)
         → 下一个请求按优先级重新选择
         ✓ 返回主实例(priority=1)
```

**恢复配置:**

```toml
[[providers.anthropic]]
name = "anthropic-primary"
priority = 1
failure_timeout_seconds = 60    # 60秒后自动恢复
```

#### 渐进式恢复(防抖动)

系统实现**渐进式恢复**以防止"抖动"(快速切换):

1. **立即故障转移**: 实例失败 → 立即切换到备份
2. **延迟返回**: 实例恢复 → 用户通过会话过期逐渐返回
3. **无强制迁移**: 现有会话保持在备份上直到自然过期
4. **渐进式负载**: 新会话转到主实例,旧会话保持在备份上

### 使用 Nginx 横向扩展

对于多机部署,使用**Nginx 一致性哈希**添加第二层粘性:

```nginx
# nginx.conf
upstream llm_gateway_cluster {
    # 基于 Authorization 头(API 密钥)的一致性哈希
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

        # LLM 请求可能运行时间较长
        proxy_read_timeout 300s;
        proxy_connect_timeout 10s;
    }
}
```

**两层粘性架构:**

```
客户端 (sk-user-alice)
    │
    ▼
Nginx 第 1 层: hash(API key) → Gateway-2
    │
    ▼
Gateway-2 第 2 层: session(API key) → Anthropic-Primary
    │
    ▼
提供商实例 (KV 缓存命中!)
```

**优势:**
- ✅ 完全无状态的网关(无跨进程通信)
- ✅ 无需 Redis/共享状态
- ✅ 极致性能(两次仅内存哈希查找)
- ✅ 易于扩展(只需在 Nginx upstream 中添加/删除网关实例)
- ✅ 故障隔离(一个网关失败不影响其他网关)

## 快速开始

### 1. 配置

**重要:** 切勿将包含真实 API 密钥的 `config.toml` 提交到版本控制!

从示例创建配置文件:

```bash
cp config.toml.example config.toml
```

然后编辑 `config.toml` 并将占位符值替换为实际的 API 密钥:

```toml
[server]
host = "0.0.0.0"
port = 8080
log_level = "info"
log_format = "json"

# API 密钥
[[api_keys]]
key = "sk-gateway-001"
name = "my-app"
enabled = true

# 模型映射(定义每个模型使用哪个提供商)
[models.gpt-4]
provider = "openai"
api_model = "gpt-4"

[models."claude-3-5-sonnet"]
provider = "anthropic"
api_model = "claude-3-5-sonnet-20241022"

[models."gemini-1.5-pro"]
provider = "gemini"
api_model = "models/gemini-1.5-pro-latest"

# 提供商配置
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

# 指标
[metrics]
enabled = true
endpoint = "/metrics"
include_api_key_hash = true
```

### 2. 使用 Docker 运行

```bash
docker build -t llm-gateway .
docker run -p 8080:8080 -v $(pwd)/config.toml:/app/config.toml llm-gateway
```

### 3. 从源代码运行

```bash
# 仅后端
cd backend
cargo run --release

# 包含前端(用于开发)
cd frontend
npm install
npm run dev        # 前端开发服务器,运行在 http://localhost:3000

# 生产构建(前端)
cd frontend
npm run build      # 构建到 frontend/dist/
cd ../backend
cargo run --release  # 从 / 提供前端服务
```

### 4. 访问仪表板

运行后,在以下地址访问 Web 仪表板:
```
http://localhost:8080/
```

仪表板提供:
- 实时 token 使用监控
- 提供商实例健康状态
- 按 API 密钥的分析和成本估算
- 请求追踪可视化

## API 端点

### 核心 LLM API

| 端点 | 方法 | 认证 | 说明 |
|----------|--------|------|-------------|
| `/v1/chat/completions` | POST | 是 | OpenAI 兼容的聊天补全(所有提供商) |
| `/v1/messages` | POST | 是 | 原生 Anthropic Messages API(仅 Claude 模型) |
| `/v1/models` | GET | 是 | 列出可用模型 |

### 监控与可观测性

| 端点 | 方法 | 认证 | 说明 |
|----------|--------|------|-------------|
| `/health` | GET | 否 | 健康检查 |
| `/ready` | GET | 否 | 就绪检查 |

### 仪表板 API(新)

| 端点 | 方法 | 认证 | 说明 |
|----------|--------|------|-------------|
| `/` | GET | 否 | Web 仪表板(Vue 3 SPA) |
| `/api/requests/time-series` | GET | 否 | Token 使用时间序列数据 |
| `/api/requests/by-api-key` | GET | 否 | 按 API 密钥的 token 聚合 |
| `/api/requests/by-instance` | GET | 否 | 按实例的 token 分布 |
| `/api/instances/health-time-series` | GET | 否 | 实例健康随时间变化 |
| `/api/instances/current-health` | GET | 否 | 当前实例健康状态 |

### 配置管理 API(新)

| 端点 | 方法 | 认证 | 说明 |
|----------|--------|------|-------------|
| `/api/config/api-keys` | GET | 否 | 列出所有 API 密钥 |
| `/api/config/api-keys` | POST | 否 | 创建新 API 密钥 |
| `/api/config/api-keys/:name` | PUT | 否 | 更新 API 密钥(启用/禁用) |
| `/api/config/api-keys/:name` | DELETE | 否 | 删除 API 密钥 |
| `/api/config/routing-rules` | GET | 否 | 列出所有路由规则 |
| `/api/config/routing-rules` | POST | 否 | 创建新路由规则 |
| `/api/config/routing-rules/:id` | PUT | 否 | 更新路由规则 |
| `/api/config/routing-rules/:id` | DELETE | 否 | 删除路由规则 |
| `/api/config/providers/:provider/instances` | GET | 否 | 列出提供商实例 |
| `/api/config/providers/:provider/instances` | POST | 否 | 创建提供商实例 |
| `/api/config/providers/:provider/instances/:name` | PUT | 否 | 更新提供商实例 |
| `/api/config/providers/:provider/instances/:name` | DELETE | 否 | 删除提供商实例 |
| `/api/config/reload` | POST | 否 | 从数据库重新加载配置 |

## 配置管理

### 基于 Web 的配置界面

访问 `http://localhost:8080/config` 上的配置管理界面,通过用户友好的 Web 界面管理网关设置。

**功能**:
- **API 密钥管理**: 创建、启用/禁用和删除网关 API 密钥
- **路由规则**: 配置模型前缀到提供商的路由(例如 "gpt-" → openai)
- **提供商实例**: 使用优先级设置管理每个提供商的多个后端实例
- **热重载**: 更改立即生效,无需重启服务器
- **Anthropic 特定设置**: 为每个实例配置提示缓存和 API 版本

**配置流程**:
```
1. 初始设置(TOML 文件)
   ↓
2. 服务器将配置加载到 SQLite 数据库
   ↓
3. 使用 Web UI 管理配置
   ↓
4. 更改保存到数据库 + 热重载
   ↓
5. 无需重启服务器!
```

**重要说明**:
- **首次运行**: 服务器从 `config.toml` 加载配置到 SQLite 数据库
- **后续运行**: 从数据库加载配置(除非数据库为空,否则忽略 TOML 文件)
- **API 密钥存储**:
  - 网关 API 密钥: SHA256 哈希用于认证
  - 提供商 API 密钥: 以明文存储(上游 API 调用所需)
- **备份**: 数据库文件位于 `./data/config.db` - 定期备份

### TOML 配置(传统/初始设置)

对于初始设置或自动化部署,仍可使用 `config.toml`:

## 使用示例

### 与 Cursor 一起使用

```bash
export OPENAI_API_BASE="http://localhost:8080/v1"
export OPENAI_API_KEY="sk-gateway-001"

# 现在 Cursor 可以使用网关中配置的任何模型
# 只需在 Cursor 的设置中更改模型名称:
# - "gpt-4" → OpenAI
# - "claude-3-5-sonnet" → Anthropic(通过转换)
# - "gemini-1.5-pro" → Gemini(通过转换)
```

### 与 Claude Code 一起使用

```bash
# 原生 Anthropic API(推荐用于 Claude Code):
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_API_KEY="sk-gateway-001"

# Claude Code 将使用原生 Anthropic 格式的 /v1/messages 端点
```

### 直接 API 调用

**选项 1: OpenAI 兼容 API**(适用于所有提供商)

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

**选项 2: 原生 Anthropic API**(仅 Claude,无转换)

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer sk-gateway-001" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "Hello!"}
    ]
  }'
```

## 可观测性与仪表板

### Web 仪表板

访问 `http://localhost:8080/` 上的仪表板,实时监控网关:

**功能**:
- **Token 使用分析**: 使用交互式图表可视化 token 消耗
- **成本估算**: 基于 token 使用和提示缓存计算成本
- **提供商健康**: 监控实例健康状态和故障转移事件
- **API 密钥细分**: 按密钥的 token 使用和成本分析
- **请求追踪**: 使用性能细分可视化请求追踪

**技术**:
- 使用 Vue 3 + TypeScript + Chart.js 构建
- 来自 SQLite 数据库的实时数据
- 使用 Tailwind CSS 的响应式设计

### 基于 SQLite 的可观测性

所有请求都记录到 SQLite 数据库(`./data/observability.db`),包含完整详细信息:

**请求数据包括**:
- 基本信息: request_id、timestamp、api_key_name、provider、instance、model、endpoint
- Token 使用: input_tokens、output_tokens、total_tokens
- **缓存指标**: cache_creation_input_tokens、cache_read_input_tokens(仅 Anthropic)
- 性能: duration_ms、status、error_type、error_message

**查询示例**:
```sql
-- 按提供商的 token 使用(最近 7 天)
SELECT provider, model,
       SUM(input_tokens) as total_input,
       SUM(output_tokens) as total_output,
       SUM(cache_read_input_tokens) as cache_savings
FROM requests
WHERE date >= date('now', '-7 days')
GROUP BY provider, model;

-- 最慢的请求(p99 延迟)
SELECT request_id, model, duration_ms, timestamp
FROM requests
ORDER BY duration_ms DESC
LIMIT 100;

-- 缓存效率(仅 Anthropic)
SELECT
    COUNT(*) as requests,
    SUM(cache_read_input_tokens) as total_cached,
    SUM(input_tokens) as total_input,
    ROUND(100.0 * SUM(cache_read_input_tokens) / SUM(input_tokens), 2) as cache_hit_rate
FROM requests
WHERE provider = 'anthropic' AND date >= date('now', '-1 day');
```

**数据保留**:
- 请求日志: 7 天(可配置)
- 追踪 spans: 7 天
- 自动清理在每天凌晨 3 点运行

所有指标都存储在 SQLite 中,可通过以下方式访问:
- **Web 仪表板**: `http://localhost:8080/` 上的实时图表
- **SQL 查询**: 直接数据库访问用于自定义分析
- **REST API**: 仪表板 API 端点用于程序化访问

## 功能矩阵

网关支持所有提供商的全面多模态功能:

| 功能 | OpenAI | Anthropic | Gemini | 说明 |
|---------|:------:|:---------:|:------:|-------|
| **文本补全** | ✅ | ✅ | ✅ | 完全支持 |
| **流式传输** | ✅ | ✅ | ✅ | SSE 实时转换 |
| **视觉/图像** | ✅ | ✅ | ✅ | 自动 base64 转换 |
| **工具调用(非流式)** | ✅ | ✅ | ✅ | 完整的请求/响应转换 |
| **工具调用(流式)** | ✅ | ✅ | ✅ | 增量 JSON 组装 |
| **提示缓存** | ❌ | ✅ | ❌ | 系统提示和工具的自动缓存 |
| **JSON 模式** | ✅ | ✅ ⚠️ | ✅ | ⚠️ = 系统提示注入变通方法 |
| **JSON Schema** | ✅ | ✅ ⚠️ | ✅ | ⚠️ = 系统提示注入变通方法 |
| **转换警告** | N/A | ✅ | ✅ | X-LLM-Gateway-Warnings 头 |

**图例:**
- ✅ = 完全原生或转换支持
- ⚠️ = 通过系统提示注入的变通方法
- ❌ = 提供商不支持

### 视觉/图像支持

使用 OpenAI 格式发送图像(适用于所有提供商):

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "这张图片里有什么?"},
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

网关自动:
- 为所有提供商转换 base64 数据 URL
- 在单个请求中处理多个图像
- 保留图像细节设置

### 工具/函数调用

使用 OpenAI 格式定义工具:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{"role": "user", "content": "天气怎么样?"}],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "获取当前天气",
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

网关转换为:
- **Anthropic**: 带有 `name`、`description`、`input_schema` 的 `tools` 数组
- **Gemini**: 带有参数 schema 的 `function_declarations`

支持:
- 自动工具选择
- 必需的工具使用
- 特定工具强制
- 带有工具结果的多轮对话
- 增量 JSON 的流式工具调用

### 提示缓存(Anthropic)

在 `config.toml` 中配置自动缓存:

```toml
[[providers.anthropic]]
name = "anthropic-primary"
# ... 其他配置 ...

[providers.anthropic.cache]
auto_cache_system = true         # 自动缓存大型系统提示
min_system_tokens = 1024          # 触发缓存的最小 tokens
auto_cache_tools = true           # 自动缓存工具定义
```

网关自动:
- 检测大型系统提示(≥1024 tokens)
- 向最后一个系统提示块添加 `cache_control`
- 缓存工具定义(在最后一个工具上标记)
- 在需要时将文本转换为块格式

**成本节省**: 缓存内容可节省约 90% 的成本!

### JSON 模式与结构化输出

请求 JSON 响应:

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{"role": "user", "content": "列出 3 种颜色"}],
  "response_format": {"type": "json_object"}
}
```

使用严格的 schema:

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

**提供商实现:**
- **OpenAI**: 原生 `response_format` 支持
- **Gemini**: 通过 `response_mime_type` 和 `response_schema` 原生支持
- **Anthropic**: 系统提示注入(检查 `X-LLM-Gateway-Warnings` 头)

### 转换警告

当参数不被原生支持时,网关通过 HTTP 头添加警告:

```http
X-LLM-Gateway-Warnings: [{"level":"warning","message":"Parameter 'seed' not supported by Anthropic provider, ignoring"}]
```

警告出现在:
- 不支持的参数(`seed`、`logprobs`、`logit_bias` 等)
- 提供商特定的变通方法(Anthropic 上的 JSON 模式)
- 功能限制

## 协议转换

网关在协议之间自动转换:

| 功能 | OpenAI | Anthropic | Gemini | 转换 |
|---------|--------|-----------|--------|------------|
| 系统消息 | `messages[0].role="system"` | `system` 字段 | `systemInstruction` | ✅ 已提取 |
| 角色名称 | `assistant` | `assistant` | `model` | ✅ 已映射 |
| max_tokens | 可选 | 必需 | 可选 | ✅ 默认值: 4096 |
| temperature | 0-2 | 0-1 | 0-2 | ✅ 已截断 |
| 内容块 | 字符串或数组 | 字符串或数组 | Parts 数组 | ✅ 已转换 |
| 工具 | OpenAI 格式 | Anthropic 格式 | 函数声明 | ✅ 已转换 |
| 图像 | URL 或 base64 | 仅 Base64 | 仅 Base64 | ✅ 自动转换 |

## 示例

代码库包含演示所有主要功能的全面示例:

```bash
# 视觉/图像支持
cargo run --example vision_example

# 工具/函数调用
cargo run --example tool_calling_example

# JSON 模式和结构化输出
cargo run --example json_mode_example

# 成本优化的提示缓存
cargo run --example caching_example
```

每个示例包括:
- 带有详细注释的工作代码
- 每个功能的多个用例
- 提供商特定说明
- 成本优化策略

## 开发

### 运行测试

```bash
# 单元测试
cargo test

# 集成测试
cargo test --test '*'

# 特定功能测试
cargo test --test multimodal_tests
cargo test --test tool_calling_tests
cargo test --test json_mode_tests
cargo test --test caching_tests
```

### 构建发布版本

#### 快速开始(macOS 开发)

```bash
# 快速开发构建(调试配置,约 30 秒-1 分钟)
cd backend
cargo build
cargo run

# macOS 生产构建
cargo build --release
# 输出: backend/target/release/llm-gateway
```

#### 跨平台编译(Linux)

项目支持交叉编译,从 macOS 构建 Linux 二进制文件。

**首次设置:**

```bash
# 1. 安装 Linux 目标
rustup target add x86_64-unknown-linux-gnu

# 2. 安装 cross 工具(需要 Docker)
cargo install cross --git https://github.com/cross-rs/cross

# 3. 完成!现在可以为 Linux 构建
```

**构建 Linux 二进制文件:**

```bash
# 选项 1: 使用构建脚本(推荐)
./scripts/build-linux.sh

# 选项 2: 直接命令(必须从项目根目录运行!)
cross build \
    --manifest-path backend/Cargo.toml \
    --target x86_64-unknown-linux-gnu \
    --release
# 输出: backend/target/x86_64-unknown-linux-gnu/release/llm-gateway

# 选项 3: 完全静态 Linux 二进制文件(无系统依赖)
cross build \
    --manifest-path backend/Cargo.toml \
    --target x86_64-unknown-linux-musl \
    --release
# 输出: backend/target/x86_64-unknown-linux-musl/release/llm-gateway
```

**重要:** 直接使用 `cross` 时,始终从**项目根目录**运行(不是 `backend` 目录),并使用 `--manifest-path backend/Cargo.toml`。这确保 `frontend/dist` 可被构建容器访问以进行嵌入。

**二进制文件大小:**
- macOS(release): 约 10MB
- Linux(release): 约 10MB
- Linux MUSL(static): 约 12MB

**故障排除:**

如果遇到 OpenSSL 相关错误,请确保使用已从 `native-tls` 切换到 `rustls`(纯 Rust SSL 实现)的最新代码。

更多文档:
- **[IMPLEMENTATION.md](docs/IMPLEMENTATION.md)** - 完整的实现细节和架构
- **[FEATURES.md](docs/FEATURES.md)** - 全面的功能文档
- **[CONVERSION_LIMITATIONS.md](docs/CONVERSION_LIMITATIONS.md)** - 提供商转换权衡
- **[DAEMON.md](docs/DAEMON.md)** - 作为守护进程/后台服务运行

## 配置

### 可观测性配置

添加到 `config.toml`:

```toml
[observability]
enabled = true
database_path = "./data/observability.db"

# 性能调优
[observability.performance]
batch_size = 100              # 每批次写入的事件数
flush_interval_ms = 100       # 刷新批次前的最长时间
max_buffer_size = 10000       # 环形缓冲区大小

# 数据保留策略
[observability.retention]
logs_days = 7                     # 保留请求日志 7 天
spans_days = 7                    # 保留追踪 spans 7 天
cleanup_hour = 3                  # 每天凌晨 3 点运行清理(0-23)
```

### 环境变量

可以使用环境变量覆盖配置:

```bash
export LLM_GATEWAY__SERVER__PORT=9000
export LLM_GATEWAY__PROVIDERS__OPENAI__API_KEY="sk-new-key"
export LLM_GATEWAY__OBSERVABILITY__ENABLED=true
```

## 许可证

MIT

## 架构细节

有关完整的架构文档,请参见代码库中的实现计划,包括:
- 三端点设计
- 模型路由逻辑
- 协议转换策略
- 流式架构
- 指标实现

使用 Axum、Tokio 和 SQLite,用 ❤️ 在 Rust 中构建。
