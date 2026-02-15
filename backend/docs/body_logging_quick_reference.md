# Body Logging 快速参考

## 配置

### 最小配置
```toml
[observability]
enabled = true

[observability.body_logging]
enabled = true
```

### 完整配置
```toml
[observability.body_logging]
enabled = true
max_body_size = 102400  # 100KB
log_level = "info"

[[observability.body_logging.redact_patterns]]
pattern = "sk-[a-zA-Z0-9]{48}"
replacement = "sk-***REDACTED***"
```

## 日志事件类型

### 1. request_body
```bash
grep "request_body" logs/requests.$(date +%Y-%m-%d) | jq '.fields'
```

### 2. response_body
```bash
grep "response_body" logs/requests.$(date +%Y-%m-%d) | jq '.fields'
```

### 3. trace_span
```bash
grep "trace_span" logs/requests.$(date +%Y-%m-%d) | jq '.fields'
```

## 常用查询

### 查询特定请求的所有事件
```bash
REQUEST_ID="your-request-id"
grep "$REQUEST_ID" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 查询某个API key的所有请求
```bash
API_KEY="client-1"
grep "\"api_key_name\":\"$API_KEY\"" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 查询失败的请求
```bash
grep "\"status\":\"failure\"" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 统计路由耗时
```bash
grep "route_model" logs/requests.$(date +%Y-%m-%d) | \
  jq '.fields.duration_ms' | \
  awk '{sum+=$1; count++} END {print "Avg:", sum/count, "ms"}'
```

### 查询被截断的响应
```bash
grep "\"truncated\":true" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 查询流式响应
```bash
grep "\"streaming\":true" logs/requests.$(date +%Y-%m-%d) | jq .
```

## 对话历史分析

### 提取某个API key的对话
```bash
API_KEY="client-1"
grep "\"api_key_name\":\"$API_KEY\"" logs/requests.* | \
  grep -E "request_body|response_body" | \
  jq -s 'group_by(.span.request_id) | .[] | {
    request_id: .[0].span.request_id,
    timestamp: .[0].timestamp,
    request: (.[0].fields.body // ""),
    response: (.[1].fields.body // "")
  }'
```

### 统计每个API key的请求数
```bash
grep "request_body" logs/requests.$(date +%Y-%m-%d) | \
  jq -r '.span.api_key_name' | \
  sort | uniq -c | sort -rn
```

## 性能分析

### 统计响应大小分布
```bash
grep "response_body" logs/requests.$(date +%Y-%m-%d) | \
  jq '.fields.body_size' | \
  awk '{
    if ($1 < 1024) small++
    else if ($1 < 10240) medium++
    else if ($1 < 102400) large++
    else xlarge++
  }
  END {
    print "< 1KB:", small
    print "1-10KB:", medium
    print "10-100KB:", large
    print "> 100KB:", xlarge
  }'
```

### 统计streaming vs non-streaming
```bash
grep "response_body" logs/requests.$(date +%Y-%m-%d) | \
  jq -r '.fields.streaming' | \
  sort | uniq -c
```

## 故障排查

### 查找错误
```bash
grep "\"level\":\"ERROR\"" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 查找超时
```bash
grep "\"status\":\"timeout\"" logs/requests.$(date +%Y-%m-%d) | jq .
```

### 查找特定错误消息
```bash
grep "error_message" logs/requests.$(date +%Y-%m-%d) | \
  jq '.fields.error_message' | \
  sort | uniq -c
```

## 日志维护

### 查看日志文件大小
```bash
ls -lh logs/requests.*
```

### 清理旧日志(手动)
```bash
find logs -name "requests.*" -mtime +7 -delete
```

### 压缩旧日志
```bash
find logs -name "requests.*" -mtime +1 -exec gzip {} \;
```

## 脱敏验证

### 检查是否有未脱敏的API key
```bash
# 应该返回空(所有key都已脱敏)
grep -E "sk-[a-zA-Z0-9]{48}" logs/requests.$(date +%Y-%m-%d) | \
  grep -v "REDACTED"
```

## 性能监控

### 监控日志写入速度
```bash
watch -n 1 'wc -l logs/requests.$(date +%Y-%m-%d)'
```

### 监控日志文件增长
```bash
watch -n 5 'ls -lh logs/requests.$(date +%Y-%m-%d)'
```

## 提示

1. **使用jq进行复杂查询**: jq是处理JSONL的最佳工具
2. **结合grep和jq**: 先用grep过滤,再用jq格式化
3. **定期清理日志**: 避免磁盘空间不足
4. **监控日志大小**: 如果日志过大,考虑调整max_body_size
5. **备份重要日志**: 在清理前备份需要长期保存的日志
