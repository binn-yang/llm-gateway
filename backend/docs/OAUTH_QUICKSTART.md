# Anthropic OAuth 快速配置指南

本指南提供 Anthropic OAuth 认证的快速配置步骤。

## 🚀 快速开始

### 1. 配置 OAuth 提供商

在 `config.toml` 中添加以下配置（**使用精确值**）:

```toml
[[oauth_providers]]
name = "anthropic"
client_id = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
auth_url = "https://claude.ai/oauth/authorize"
token_url = "https://console.anthropic.com/v1/oauth/token"
redirect_uri = "https://platform.claude.com/oauth/code/callback"
scopes = [
  "org:create_api_key",
  "user:profile",
  "user:inference",
  "user:sessions:claude_code"
]
```

### 2. 配置提供商实例

```toml
[[providers.anthropic]]
name = "anthropic-oauth"
enabled = true
auth_mode = "oauth"
oauth_provider = "anthropic"
base_url = "https://api.anthropic.com/v1"
timeout_seconds = 300
api_version = "2023-06-01"
priority = 1
failure_timeout_seconds = 60
```

### 3. 运行 OAuth 登录

```bash
# 构建 release 版本
cargo build --release

# 执行 OAuth 登录
./target/release/llm-gateway oauth login anthropic
```

### 4. 完成认证流程

1. **浏览器自动打开**（或复制显示的 URL）
2. **授权页面** - 点击"Allow"授予权限
3. **浏览器跳转** 到 `https://platform.claude.com/oauth/code/callback?code=xxx&state=yyy`
4. **复制完整 URL** - 从浏览器地址栏复制整个 URL
5. **粘贴到 CLI** - 在命令行提示符处粘贴 URL 并按回车
6. **完成！** - Token 自动保存到 `~/.llm-gateway/oauth_tokens.json`

### 5. 验证 Token 状态

```bash
# 检查 token 状态
./target/release/llm-gateway oauth status anthropic

# 查看详细信息
./target/release/llm-gateway oauth status anthropic -v
```

### 6. 启动网关

```bash
./target/release/llm-gateway start
```

## 📝 关键配置说明

### 必须使用的精确值

| 参数 | 必须值 | 说明 |
|------|--------|------|
| `client_id` | `9d1c250a-e61b-44d9-88ed-5944d1962f5e` | 官方 Anthropic OAuth client ID (UUID 格式) |
| `auth_url` | `https://claude.ai/oauth/authorize` | 使用 claude.ai 域名（**不是** console.anthropic.com） |
| `token_url` | `https://console.anthropic.com/v1/oauth/token` | 包含 `/v1` 路径 |
| `redirect_uri` | `https://platform.claude.com/oauth/code/callback` | 官方远程回调地址 |

### 自动添加的参数

Gateway 会自动在授权 URL 中添加以下参数:
- `code=true` - Anthropic 必需参数
- `code_challenge` - PKCE challenge
- `code_challenge_method=S256` - PKCE 方法
- `scope` - 将数组转为空格分隔字符串

## ❌ 常见错误

### 错误配置示例

```toml
# ❌ 错误 1: 旧的/错误的 client_id
client_id = "claude-code-cli"  # 不工作！

# ❌ 错误 2: 错误的 auth_url 域名
auth_url = "https://console.anthropic.com/oauth/authorize"  # 404!

# ❌ 错误 3: token_url 缺少 /v1
token_url = "https://console.anthropic.com/oauth/token"  # 404!

# ❌ 错误 4: 使用 localhost redirect
redirect_uri = "http://localhost:54545/callback"  # 官方 client_id 不支持!

# ❌ 错误 5: 不完整的 scopes
scopes = ["api"]  # 权限不足!
```

## 🔧 故障排查

### "Token exchange failed" (401/404)
- **原因**: `token_url` 错误
- **解决**: 确保使用 `https://console.anthropic.com/v1/oauth/token`（包含 `/v1`）

### "State parameter mismatch"
- **原因**: 复制的 URL 不完整或错误
- **解决**: 确保复制完整的 URL，包括 `?code=xxx&state=yyy`

### "Invalid callback URL domain"
- **原因**: 粘贴了非 Anthropic 域名的 URL
- **解决**: 只粘贴 `claude.com` 或 `anthropic.com` 域名的 URL

### "Client authentication failed"
- **原因**: 错误的 `client_id`
- **解决**: 使用官方 client ID: `9d1c250a-e61b-44d9-88ed-5944d1962f5e`

### "Authorization failed" (浏览器错误)
- **原因**: 错误的 `auth_url`
- **解决**: 使用 `https://claude.ai/oauth/authorize`（不是 console.anthropic.com）

## 🔐 Token 管理

### Token 存储位置
```
~/.llm-gateway/oauth_tokens.json
```

### Token 加密
- 使用 AES-256-GCM 加密
- 机器特定的加密密钥（不可移植）
- 包含 access_token 和 refresh_token

### Token 生命周期
- **过期时间**: 通常 1 小时
- **自动刷新**:
  - 后台任务: 每 5 分钟检查，刷新 < 10 分钟到期的 token
  - 按需刷新: 每次请求前检查，刷新 < 1 分钟到期的 token

### 手动操作
```bash
# 查看状态
llm-gateway oauth status anthropic

# 手动刷新
llm-gateway oauth refresh anthropic

# 登出（删除 token）
llm-gateway oauth logout anthropic
```

## 📊 验证授权 URL

正确的授权 URL 应该包含以下参数:

```
https://claude.ai/oauth/authorize?
  client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&
  redirect_uri=https://platform.claude.com/oauth/code/callback&
  response_type=code&
  code_challenge=<random>&
  code_challenge_method=S256&
  state=<random>&
  scope=org:create_api_key+user:profile+user:inference+user:sessions:claude_code&
  code=true
```

## 🧪 测试授权 URL 生成

运行测试以验证配置:

```bash
cargo test test_anthropic_oauth_url_generation -- --nocapture
```

输出应显示:
```
✓ All URL parameters are correct!
✓ client_id: 9d1c250a-e61b-44d9-88ed-5944d1962f5e
✓ auth_url: https://claude.ai/oauth/authorize
✓ code=true parameter present
✓ PKCE parameters correct
✓ Scopes: org:create_api_key user:profile user:inference user:sessions:claude_code
```

## 📚 更多信息

详细的架构说明和实现细节请参阅:
- [CLAUDE.md](./CLAUDE.md) - 完整的项目文档
- [config.toml.example](./config.toml.example) - 配置模板

## ⚠️ 重要提醒

1. **不要修改配置值** - 使用精确的官方值
2. **完整复制 URL** - 必须包含 `?code=xxx&state=yyy` 参数
3. **不要共享 token** - Token 文件包含敏感信息
4. **定期检查状态** - 确保 token 未过期
5. **遇到问题重新登录** - `llm-gateway oauth logout` 然后重新 `login`
