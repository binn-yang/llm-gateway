# LLM Gateway 压力测试文档

本目录包含 LLM Gateway 的完整压力测试套件，用于验证网关的性能、稳定性和正确性。

## 测试架构

```
压力测试体系
├── 集成测试 (tests/stress_scenarios.rs)
│   ├── Mock 基础设施 (tests/integration/mocks/)
│   ├── 配置生成器 (tests/integration/helpers/test_config.rs)
│   └── 指标收集器 (tests/integration/helpers/metrics.rs)
├── 基准测试 (benches/)
│   ├── load_balancer_bench.rs - DashMap 性能
│   └── conversion_bench.rs - 协议转换性能
└── 运行脚本 (tests/stress/)
    └── run_stress_tests.sh - 自动化运行
```

## 8 个核心测试场景

### 快速测试 (< 5 秒)

#### 场景 1B: Mock 基准测试
- **目标**: 验证 Mock 基础设施性能
- **配置**: 100 个顺序请求, 0ms 延迟
- **成功标准**: P99 < 50ms, 成功率 > 99%
- **运行**: `./run_stress_tests.sh --scenario 1b`

#### 场景 2B: 流式响应基准
- **目标**: 验证 SSE 流式响应
- **配置**: 10 chunks, 20ms 间隔
- **成功标准**: TTFB < 150ms, 格式正确
- **运行**: `./run_stress_tests.sh --scenario 2b`

#### 场景 5: 协议转换开销
- **目标**: 测量 OpenAI ↔ Anthropic 转换成本
- **配置**: 100 个请求对比测试
- **成功标准**: 转换开销 < 2ms
- **运行**: `./run_stress_tests.sh --scenario 5`
- **实测结果**: ~78μs 转换开销

#### 场景 6: 流式吞吐量
- **目标**: 并发流式响应测试
- **配置**: 10 个并发连接, 100 chunks
- **成功标准**: P99 < 2s, 成功率 > 99%
- **运行**: `./run_stress_tests.sh --scenario 6`

#### 场景 7: 故障转移
- **目标**: 验证自动 failover
- **配置**: Primary (100% 错误) + Backup (正常)
- **成功标准**: Failover < 10ms, 成功率 > 90%
- **运行**: `./run_stress_tests.sh --scenario 7`

### 中等时间测试 (1-3 分钟)

#### 场景 3: 粘性会话缓存
- **目标**: 验证 DashMap 会话缓存有效性
- **配置**: 100 个 API key × 100 个请求 = 10,000 请求
- **成功标准**: 缓存命中率 > 99%, P99 < 50ms
- **运行**: `./run_stress_tests.sh --scenario 3`
- **实测结果**: 100% 成功率, P99 = 13ms, QPS = 80

### 长时间测试 (> 5 分钟)

#### 场景 2: 并发吞吐量
- **目标**: 最大 QPS 和并发瓶颈识别
- **配置**: 1000 并发 × 100 请求, 100ms mock 延迟
- **成功标准**: QPS > 5000, CPU < 80%, 无失败
- **运行**: `./run_stress_tests.sh --scenario 2`
- **预计时间**: ~10 分钟

#### 场景 4: 负载均衡分布
- **目标**: 验证加权随机选择均匀性
- **配置**: 3 实例 (权重 100, 200, 100), 10,000 个不同 API key
- **成功标准**: 分布 25%, 50%, 25% (±5% 容差)
- **运行**: `./run_stress_tests.sh --scenario 4`
- **预计时间**: ~5 分钟

#### 场景 8: 内存泄漏检测
- **目标**: 长时间运行稳定性测试
- **配置**: 30 分钟, QPS 100, 50% 流式
- **成功标准**: RSS 增长 < 10MB, 成功率 > 95%
- **运行**: `./run_stress_tests.sh --scenario 8`
- **预计时间**: 30 分钟
- **警告**: 需要手动中止 (Ctrl+C)

## 快速开始

### 运行所有快速测试
```bash
cd backend/tests/stress
./run_stress_tests.sh
```

预计时间: ~3 分钟
包含场景: 1B, 2B, 3, 5, 6, 7

### 运行所有测试 (包括长时间测试)
```bash
./run_stress_tests.sh --all
```

预计时间: ~20 分钟
包含场景: 1B, 2, 2B, 3, 4, 5, 6, 7
**不包含**: 场景 8 (30 分钟) - 需单独运行

### 运行特定场景
```bash
./run_stress_tests.sh --scenario 3
```

### 运行 Criterion 基准测试
```bash
./run_stress_tests.sh --bench
```

查看 HTML 报告: `target/criterion/index.html`

## 直接使用 Cargo

### 运行单个测试
```bash
cd backend

# 快速测试
cargo test --test stress_scenarios test_scenario_1b_mock_baseline -- --nocapture

# 长时间测试 (需要 --ignored)
cargo test --test stress_scenarios test_scenario_2_concurrent_throughput -- --nocapture --ignored
```

### 运行所有集成测试
```bash
# 仅快速测试
cargo test --test stress_scenarios

# 包括长时间测试
cargo test --test stress_scenarios -- --include-ignored
```

### 运行基准测试
```bash
# 所有基准
cargo bench

# 特定基准
cargo bench --bench load_balancer_bench

# 保存 baseline (用于回归检测)
cargo bench -- --save-baseline main

# 与 baseline 对比
cargo bench -- --baseline main
```

## 性能指标基准

| 指标 | 目标值 | 实测值 | 状态 |
|------|--------|--------|------|
| 网关开销 P99 | < 10ms | - | 待测 |
| Mock P99 | < 50ms | 13ms | ✅ |
| 协议转换开销 | < 2ms | 78μs | ✅ |
| 并发 QPS | > 5000 | - | 待测 |
| 粘性会话命中率 | > 99% | 100% | ✅ |
| 流式 TTFB | < 100ms | ~55ms | ✅ |
| 成功率 | > 99% | 100% | ✅ |

## 测试输出示例

```
========== Scenario 3: Sticky Session Cache Hit Rate Test ==========
Testing 100 API keys with 100 requests each
Total requests: 10000
  Progress: 100/100 API keys
All requests completed in 123.78s

========== Stress Test Metrics Report ==========
Total Requests:      10000
Successful:          10000 (100.00%)
Failed:              0

Latency (successful requests only):
  Min:               10.46ms
  Avg:               12.37ms
  P50:               12.50ms
  P95:               12.75ms
  P99:               13.11ms
  Max:               19.90ms

Throughput:
  QPS:               80.77
  Total Duration:    123.80s
=================================================

✓ Scenario 3 PASSED
```

## 故障排查

### 编译错误
```bash
# 清理并重新编译
cargo clean
cargo test --test stress_scenarios --no-run
```

### 测试超时
某些测试可能需要较长时间:
- 场景 3: ~2 分钟 (10,000 请求)
- 场景 4: ~5 分钟 (10,000 请求 × 3 实例)
- 场景 8: 30 分钟 (长时间运行)

### 端口冲突
Mock 服务器使用随机可用端口，通常不会冲突。

### 内存不足
场景 2 (1000 并发) 和场景 8 (长时间运行) 可能需要较多内存 (建议 > 4GB)。

## CI/CD 集成

在 GitHub Actions 中运行:
```yaml
- name: Run stress tests
  run: |
    cd backend/tests/stress
    ./run_stress_tests.sh  # 仅快速测试
```

完整配置参考: `.github/workflows/stress_tests.yml` (待创建)

## 内存分析工具

### Linux: heaptrack
```bash
heaptrack ./target/release/llm-gateway start &
cd backend/tests/stress
./run_stress_tests.sh
kill %1  # 停止 gateway
heaptrack --analyze heaptrack.llm-gateway.*.gz
```

### macOS: Instruments
```bash
instruments -t Allocations ./target/release/llm-gateway start
# 在另一个终端运行测试
```

## 扩展测试

### 添加新场景
1. 在 `tests/stress_scenarios.rs` 中添加测试函数
2. 使用 `#[tokio::test]` 标注
3. 使用 `#[ignore]` 标注长时间测试
4. 在 `run_stress_tests.sh` 中添加场景入口

### 自定义 Mock
参考 `tests/integration/mocks/openai_mock.rs`:
```rust
pub async fn setup_custom_mock(
    latency_ms: u64,
    error_rate: f64,
) -> MockServer {
    let mock_server = MockServer::start().await;
    // ... 配置 mock
    mock_server
}
```

### 自定义指标
参考 `tests/integration/helpers/metrics.rs`:
```rust
let metrics = StressTestMetrics::new();
// ... 记录请求
let report = metrics.report();
report.assert_performance(99.0, Duration::from_millis(50), 0.0);
```

## 注意事项

1. **Mock 限制**: 当前测试使用 Mock,未启动真实网关
   - 无法测试完整的 LoadBalancer 逻辑
   - 无法测试真实的协议转换开销
   - 需要集成真实网关才能进行端到端测试

2. **发布模式**: 性能测试应使用 `--release` 构建
   ```bash
   cargo test --test stress_scenarios --release
   ```

3. **并发限制**: 系统文件描述符限制可能影响高并发测试
   ```bash
   # macOS/Linux 提高限制
   ulimit -n 10000
   ```

4. **磁盘空间**: Criterion 基准测试会生成大量报告文件 (~100MB)
   ```bash
   # 清理旧报告
   rm -rf target/criterion
   ```

## 下一步

- [ ] 集成真实网关服务器
- [ ] 实现完整的 LoadBalancer 测试
- [ ] 添加 wrk HTTP 负载测试脚本
- [ ] 创建 GitHub Actions workflow
- [ ] 实现 Criterion 基准测试
- [ ] 添加火焰图生成脚本

## 许可证

与主项目相同
