# LLM Gateway 监控数据优化 - 测试指南

## 概述

本文档描述如何测试和验证 SQLite 监控数据优化功能。

## 前提条件

1. 后端已编译：`cd backend && cargo build --release`
2. 前端已构建：`cd frontend && npm install`
3. 后端服务运行中：`./target/release/llm-gateway start`
4. 前端服务运行在：http://localhost:3000/

---

## Phase 1: 数据库迁移测试

### 1.1 验证数据库表创建

```bash
# 连接到 SQLite 数据库
sqlite3 data/observability.db

# 查看表结构
.schema token_usage
.schema instance_health
.schema hourly_metrics

# 验证索引
.indexes token_usage
.indexes instance_health

# 退出
.quit
```

**预期结果**：
- ✅ 3 张表已创建：`token_usage`, `instance_health`, `hourly_metrics`
- ✅ 索引已创建
- ✅ WAL 模式已启用

---

## Phase 2: 数据写入测试

### 2.1 启动后端并验证初始化

```bash
# 启动后端
cd backend
./target/release/llm-gateway start

# 查看日志（应该看到）
# Initializing observability database
# Running database migrations...
# Starting metrics snapshot task (60s interval)
```

### 2.2 发送测试请求生成数据

```bash
# 发送多个请求以生成 metrics
for i in {1..10}; do
  curl -X POST http://localhost:8080/v1/messages \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer y111" \
    -d '{
      "model": "claude-3-5-haiku-20241022",
      "max_tokens": 50,
      "messages": [{"role": "user", "content": "Test '"$i"'}]
    }' &
  sleep 0.5
done
```

### 2.3 验证数据已写入 SQLite

```bash
sqlite3 data/observability.db

# 查询 token_usage 表
SELECT * FROM token_usage ORDER BY timestamp DESC LIMIT 10;

# 查询实例健康状态
SELECT * FROM instance_health ORDER BY timestamp DESC LIMIT 10;

# 按 provider 分组统计
SELECT provider, SUM(total_tokens) as tokens
FROM token_usage
GROUP BY provider;

# 退出
.quit
```

**预期结果**：
- ✅ 数据已写入 token_usage 表
- ✅ 数据包含正确的 api_key, provider, model 字段
- ✅ timestamp 分钟级对齐
- ✅ 健康状态已记录

---

## Phase 3: API 端点测试

### 3.1 测试 Token 时序查询 API

```bash
# 查询过去 7 天的 Token 使用趋势（按 provider 分组）
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-12&end_date=2026-01-19&group_by=provider&interval=day" | jq .

# 预期返回 JSON 格式：
{
  "start_date": "2026-01-12",
  "end_date": "2026-01-19",
  "group_by": "provider",
  "interval": "day",
  "data": [
    {
      "label": "anthropic",
      "timestamp": "2026-01-12T00:00:00",
      "value": {"tokens": 12345, "requests": 10}
    }
  ]
}
```

### 3.2 测试健康状态时序 API

```bash
# 查询实例健康状态
curl "http://localhost:8080/api/dashboard/timeseries/health?start_date=2026-01-12&end_date=2026-01-19" | jq .

# 预期返回 JSON 格式：
{
  "start_date": "2026-01-12",
  "end_date": "2026-01-19",
  "data": [
    {
      "provider": "anthropic",
      "instance": "anthropic-primary",
      "timestamp": "2026-01-12T00:00:00",
      "health_status": "healthy",
      "failover_count": 0
    }
  ]
}
```

### 3.3 测试不同的分组参数

```bash
# 按 model 分组
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-12&group_by=model&interval=day" | jq .

# 按 api_key 分组
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-12&group_by=api_key&interval=day" | jq .

# 按小时粒度
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-19&group_by=provider&interval=hour" | jq .
```

---

## Phase 4: 前端测试

### 4.1 访问 Dashboard

1. 打开浏览器访问：http://localhost:3000/

2. 验证以下组件显示正常：
   - ✅ SummaryCards（6 个指标卡片）
   - ✅ TokenUsageChart（柱状图）
   - ✅ ProviderHealthChart（实例网格）
   - ✅ **TokenUsageTimeseries**（新增折线图）
   - ✅ **InstanceHealthTimeseries**（新增健康曲线）

### 4.2 测试时间序列图表交互

**Token Usage Trend 图表**：
1. 尝试切换 "By Provider" / "By Model" / "By API Key" / "By Instance"
2. 尝试切换 "Daily" / "Hourly" 粒度
3. 验证图表数据每 30 秒自动刷新
4. 验证鼠标悬停显示 tooltip

**Instance Health Status 图表**：
1. 尝试从下拉框选择特定实例
2. 验证健康状态曲线（绿色 = healthy, 红色 = unhealthy）
3. 验证统计数据（Uptime, Failovers, Current Status）

### 4.3 验证数据一致性

前端显示的数据应与后端 API 返回的数据一致：
- 图表时间轴与数据库 timestamp 一致
- Token 数量与 API 返回值一致
- 分组逻辑正确（按 provider/model 等）

---

## Phase 5: 性能验证

### 5.1 写入性能测试

```bash
# 发送 1000 个请求
for i in {1..1000}; do
  curl -X POST http://localhost:8080/v1/messages \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer y111" \
    -d '{
      "model": "claude-3-5-haiku-20241022",
      "max_tokens": 10,
      "messages": [{"role": "user", "content": "Hi"}]
    }' > /dev/null 2>&1 &
done

# 等待 60 秒让数据写入
sleep 60

# 检查数据库大小
ls -lh data/observability.db

# 检查数据行数
sqlite3 data/observability.db "SELECT COUNT(*) FROM token_usage;"
```

**性能目标**：
- ✅ 写入性能：> 1000 条/秒
- ✅ 60 秒内完成 1000 个请求的快照
- ✅ 数据库大小合理（< 10MB for 1000 requests）

### 5.2 查询性能测试

```bash
# 测试大范围查询（90 天）
curl -w "@-" -o /dev/null -s "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2023-10-21&end_date=2026-01-19&group_by=provider&interval=day" \
  -H "X-Time: %{time_total}s"
```

**性能目标**：
- ✅ 7 天查询：< 100ms
- ✅ 30 天查询：< 200ms
- ✅ 90 天查询：< 500ms

---

## Phase 6: 错误处理测试

### 6.1 测试错误场景

**1. 无效日期格式**
```bash
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=invalid-date"
# 预期：400 Bad Request 或错误信息
```

**2. Observability 未启用**
```bash
# 修改 config.toml 设置 enabled = false
# 重启服务
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-19"
# 预期：错误信息 "Observability not enabled"
```

**3. 空数据集**
```bash
# 查询未来日期（无数据）
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2099-01-01&end_date=2099-01-02"
# 预期：空数组 data: []
```

---

## Phase 7: 数据保留测试

### 7.1 验证数据保留策略

```bash
# 检查 retention_policy 表
sqlite3 data/observability.db "SELECT * FROM retention_policy;"

# 预期结果：
# token_usage | 90
# instance_health | 90
# hourly_metrics | 365
# logs | 7
# spans | 7
```

### 7.2 手动清理过期数据（可选）

```bash
sqlite3 data/observability.db <<EOF
DELETE FROM token_usage WHERE date < date('now', '-90 days');
DELETE FROM instance_health WHERE date < date('now', '-90 days');
VACUUM;
EOF
```

---

## Phase 8: 端到端集成测试

### 8.1 完整测试流程

```bash
# 1. 清空现有数据（可选）
rm data/observability.db

# 2. 启动后端
cd backend
./target/release/llm-gateway start

# 3. 等待初始化完成
sleep 5

# 4. 发送测试流量
./scripts/test_traffic.sh  # 或手动发送请求

# 5. 等待 60 秒让数据写入
sleep 60

# 6. 验证数据已写入
sqlite3 data/observability.db "SELECT COUNT(*) FROM token_usage;"

# 7. 测试 API 端点
curl "http://localhost:8080/api/dashboard/timeseries/tokens?start_date=2026-01-19&group_by=provider" | jq .

# 8. 测试前端
# 访问 http://localhost:3000/ 并验证图表显示
```

---

## 常见问题排查

### 问题 1: API 返回 "Observability not enabled"

**原因**：observability.enabled = false 或数据库连接失败

**解决**：
```bash
# 检查配置
grep "enabled" config.toml | grep observability -A 1

# 检查后端日志
tail -20 /tmp/llm-gateway.log | grep observability
```

### 问题 2: 图表显示 "No data"

**原因**：
- SQLite 表为空（没有发送请求）
- 查询日期范围无数据
- 数据格式解析错误

**解决**：
```bash
# 检查表是否有数据
sqlite3 data/observability.db "SELECT COUNT(*) FROM token_usage;"

# 检查日志
tail -50 /tmp/llm-gateway.log | grep -i "snapshot\|error"

# 检查浏览器控制台
# F12 -> Console 查看错误信息
```

### 问题 3: 图表数据不刷新

**原因**：
- 轮询间隔未到
- API 错误被静默
- 缓存问题

**解决**：
```bash
# 检查网络请求
# F12 -> Network -> 找到 /timeseries/tokens -> 查看 Response

# 手动刷新页面
# Ctrl+Shift+R 强制刷新
```

---

## 验收标准

### 功能完整性
- ✅ SQLite 数据库表正确创建
- ✅ Prometheus metrics 自动写入 SQLite
- ✅ 时序查询 API 返回正确数据
- ✅ 前端图表正常显示时序数据
- ✅ 支持多维度分组（provider/model/api_key/instance）
- ✅ 支持时间范围查询

### 性能指标
- ✅ 写入性能：> 1000 条/秒
- ✅ 7 天查询：< 100ms
- ✅ 90 天查询：< 500ms
- ✅ 数据库大小：< 500MB（90 天数据）

### 用户体验
- ✅ 图表自动刷新（30 秒）
- ✅ 响应式布局适配
- ✅ 工业风格设计一致
- ✅ 错误提示友好

---

## 完成确认

完成以上所有测试后，标记以下项目为完成：

- [x] Phase 1: 数据库迁移
- [x] Phase 2: 数据写入机制
- [x] Phase 3: API 端点实现
- [x] Phase 4: 前端组件开发
- [x] Phase 5: 性能优化和测试

**恭喜！LLM Gateway SQLite 监控数据优化功能已成功实现。**
