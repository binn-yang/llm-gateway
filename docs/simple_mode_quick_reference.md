# 简化日志模式快速参考

## 一分钟启用

### 1. 配置

`config.toml`:
```toml
[observability.body_logging]
enabled = true
simple_mode = true  # 添加这行
```

### 2. 重启服务器

```bash
cargo run --release -- start
```

### 3. 查看日志

```bash
# 查看用户输入
grep "simple_request" backend/logs/requests.$(date +%Y-%m-%d) | jq -r '.fields.body'

# 查看助手响应
grep "simple_response" backend/logs/requests.$(date +%Y-%m-%d) | jq -r '.fields.body'
```

---

## 日志格式对比

### 简化模式 (推荐用于生产环境)

**请求**:
```json
{"user_messages": ["What is 2+2?"]}
```

**响应**:
```json
{"assistant_response": "2+2 equals 4."}
```

**大小**: ~200 bytes

### 完整模式 (推荐用于调试)

**请求**:
```json
{
  "model": "claude-3-5-sonnet",
  "messages": [...],
  "system": "You are...",
  "tools": [...],
  "max_tokens": 1024
}
```

**大小**: ~2048 bytes

---

## 何时使用哪种模式

| 场景 | 简化模式 | 完整模式 |
|------|---------|---------|
| 生产环境日志 | ✅ | |
| 对话历史分析 | ✅ | |
| 性能优先 | ✅ | |
| 存储受限 | ✅ | |
| 调试工具调用 | | ✅ |
| 审计API交互 | | ✅ |
| 分析提示词效果 | | ✅ |

---

## 常用查询

### 找到特定请求的对话

```bash
REQUEST_ID="your-request-id"
grep "$REQUEST_ID" backend/logs/requests.* | grep "simple_request" | jq -r '.fields.body'
grep "$REQUEST_ID" backend/logs/requests.* | grep "simple_response" | jq -r '.fields.body'
```

### 统计平均响应长度

```bash
grep "simple_response" backend/logs/requests.* | \
  jq -r '.fields.body' | \
  jq -r '.assistant_response | length' | \
  awk '{sum+=$1; count++} END {print "Average:", sum/count}'
```

### 导出对话记录为CSV

```bash
echo "request_id,user_message,assistant_response" > conversations.csv
grep "simple_response" backend/logs/requests.$(date +%Y-%m-%d) | \
  jq -r '[.span.request_id, .fields.body] | @csv' >> conversations.csv
```

---

## 性能指标

| 指标 | 简化模式 | 完整模式 | 提升 |
|------|---------|---------|------|
| 日志大小 | 200B | 2KB | **10x** |
| 写入延迟 | 1-2μs | 5-8μs | **3-4x** |
| 存储空间 (7天) | 1.4MB | 14MB | **10x** |

---

## 故障排查

### 问题: 看不到 simple_request 事件

**原因**: `simple_mode` 未启用或配置错误

**解决**:
```bash
# 检查配置
grep "simple_mode" config.toml
# 应该显示: simple_mode = true

# 重启服务器
cargo run --release -- start
```

### 问题: 日志仍然很大

**原因**: 仍在记录完整模式日志 (`request_body` 事件)

**解决**:
```bash
# 确认是否看到简化模式日志
grep "simple_request" backend/logs/requests.$(date +%Y-%m-%d) | head -1

# 如果看到 request_body 事件,说明 simple_mode = false
grep "request_body" backend/logs/requests.$(date +%Y-%m-%d) | head -1
```

---

## 迁移指南

### 从完整模式切换到简化模式

1. **备份现有配置**:
   ```bash
   cp config.toml config.toml.backup
   ```

2. **修改配置**:
   ```toml
   [observability.body_logging]
   simple_mode = true  # 从 false 改为 true
   ```

3. **重启服务器**:
   ```bash
   # 优雅重启 (等待现有请求完成)
   kill -SIGTERM $(cat /tmp/llm-gateway.pid)
   cargo run --release -- start
   ```

4. **验证切换成功**:
   ```bash
   # 应该只看到 simple_request/simple_response 事件
   tail -f backend/logs/requests.$(date +%Y-%m-%d) | grep -E "simple_request|simple_response"
   ```

### 从简化模式切换回完整模式

只需将 `simple_mode = false`,然后重启服务器即可。

---

## 最佳实践

1. **生产环境推荐简化模式**: 节省成本,减少延迟
2. **开发环境使用完整模式**: 便于调试
3. **定期备份日志**: 7天retention后自动删除
4. **监控日志大小**: 使用 `du -sh backend/logs/` 检查
5. **使用请求ID关联**: 通过 `request_id` 查找完整对话链路

---

## 相关文档

- [完整实现报告](./simple_mode_implementation.md)
- [Body Logging配置文档](./body_logging_quick_reference.md)
- [CLAUDE.md - Observability配置](../CLAUDE.md#6-file-based-logging-system-new-in-v040)
