#!/bin/bash
#
# LLM Gateway 压力测试运行脚本
#
# 使用方法:
#   ./run_stress_tests.sh              # 运行所有快速测试
#   ./run_stress_tests.sh --all        # 运行所有测试(包括长时间测试)
#   ./run_stress_tests.sh --scenario N # 运行特定场景
#   ./run_stress_tests.sh --bench      # 运行 Criterion 基准测试

set -e  # 遇到错误立即退出

# 颜色定义
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 获取脚本所在目录
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$SCRIPT_DIR/../.."

cd "$PROJECT_ROOT"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  LLM Gateway 压力测试套件${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# 解析命令行参数
RUN_ALL=false
RUN_SCENARIO=""
RUN_BENCH=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --all)
            RUN_ALL=true
            shift
            ;;
        --scenario)
            RUN_SCENARIO="$2"
            shift 2
            ;;
        --bench)
            RUN_BENCH=true
            shift
            ;;
        --help)
            echo "使用方法:"
            echo "  $0              # 运行所有快速测试"
            echo "  $0 --all        # 运行所有测试(包括长时间测试)"
            echo "  $0 --scenario N # 运行场景 N (1-8)"
            echo "  $0 --bench      # 运行 Criterion 基准测试"
            exit 0
            ;;
        *)
            echo -e "${RED}错误: 未知参数 $1${NC}"
            echo "使用 --help 查看帮助"
            exit 1
            ;;
    esac
done

# 运行特定场景
if [ -n "$RUN_SCENARIO" ]; then
    echo -e "${YELLOW}运行场景 $RUN_SCENARIO...${NC}"

    case $RUN_SCENARIO in
        1)
            cargo test --test stress_scenarios test_scenario_1_baseline_latency --release -- --nocapture
            ;;
        1b)
            cargo test --test stress_scenarios test_scenario_1b_mock_baseline --release -- --nocapture
            ;;
        2)
            cargo test --test stress_scenarios test_scenario_2_concurrent_throughput --release -- --nocapture --ignored
            ;;
        2b)
            cargo test --test stress_scenarios test_scenario_2b_streaming_baseline --release -- --nocapture
            ;;
        3)
            cargo test --test stress_scenarios test_scenario_3_sticky_session_cache_hit_rate --release -- --nocapture
            ;;
        4)
            cargo test --test stress_scenarios test_scenario_4_load_balancing_distribution --release -- --nocapture --ignored
            ;;
        5)
            cargo test --test stress_scenarios test_scenario_5_protocol_conversion_overhead --release -- --nocapture
            ;;
        6)
            cargo test --test stress_scenarios test_scenario_6_streaming_response_throughput --release -- --nocapture
            ;;
        7)
            cargo test --test stress_scenarios test_scenario_7_instance_failover --release -- --nocapture
            ;;
        8)
            cargo test --test stress_scenarios test_scenario_8_memory_leak_detection --release -- --nocapture --ignored
            ;;
        *)
            echo -e "${RED}错误: 无效的场景编号 $RUN_SCENARIO${NC}"
            echo "有效的场景: 1, 1b, 2, 2b, 3, 4, 5, 6, 7, 8"
            exit 1
            ;;
    esac

    exit 0
fi

# 运行 Criterion 基准测试
if [ "$RUN_BENCH" = true ]; then
    echo -e "${YELLOW}运行 Criterion 基准测试...${NC}"
    echo ""

    echo -e "${BLUE}[1/2] Load Balancer 基准测试${NC}"
    cargo bench --bench load_balancer_bench

    echo ""
    echo -e "${BLUE}[2/2] 协议转换基准测试${NC}"
    cargo bench --bench conversion_bench

    echo ""
    echo -e "${GREEN}✓ 所有基准测试完成${NC}"
    echo -e "${YELLOW}查看详细报告: target/criterion/index.html${NC}"
    exit 0
fi

# 运行快速测试
echo -e "${YELLOW}运行快速集成测试...${NC}"
echo ""

# 快速测试列表
FAST_TESTS=(
    "test_scenario_1b_mock_baseline"
    "test_scenario_2b_streaming_baseline"
    "test_scenario_5_protocol_conversion_overhead"
    "test_scenario_6_streaming_response_throughput"
    "test_scenario_7_instance_failover"
)

PASSED=0
FAILED=0
SKIPPED=0

for test_name in "${FAST_TESTS[@]}"; do
    echo -e "${BLUE}运行: $test_name${NC}"

    if cargo test --test stress_scenarios "$test_name" --release -- --nocapture; then
        echo -e "${GREEN}✓ PASSED${NC}"
        ((PASSED++))
    else
        echo -e "${RED}✗ FAILED${NC}"
        ((FAILED++))
    fi
    echo ""
done

# 运行中等时间测试 (场景 3)
echo -e "${BLUE}运行中等时间测试 (场景 3)...${NC}"
if cargo test --test stress_scenarios test_scenario_3_sticky_session_cache_hit_rate --release -- --nocapture; then
    echo -e "${GREEN}✓ PASSED${NC}"
    ((PASSED++))
else
    echo -e "${RED}✗ FAILED${NC}"
    ((FAILED++))
fi
echo ""

# 如果指定 --all,运行长时间测试
if [ "$RUN_ALL" = true ]; then
    echo -e "${YELLOW}运行长时间测试 (场景 2, 4, 8)...${NC}"
    echo -e "${RED}警告: 这些测试需要较长时间运行${NC}"
    echo ""

    LONG_TESTS=(
        "test_scenario_2_concurrent_throughput"
        "test_scenario_4_load_balancing_distribution"
    )

    for test_name in "${LONG_TESTS[@]}"; do
        echo -e "${BLUE}运行: $test_name${NC}"

        if cargo test --test stress_scenarios "$test_name" --release -- --nocapture --ignored; then
            echo -e "${GREEN}✓ PASSED${NC}"
            ((PASSED++))
        else
            echo -e "${RED}✗ FAILED${NC}"
            ((FAILED++))
        fi
        echo ""
    done

    # 场景 8 (30 分钟)
    echo -e "${YELLOW}跳过场景 8 (内存泄漏检测 - 30 分钟)${NC}"
    echo -e "${YELLOW}使用以下命令单独运行:${NC}"
    echo -e "  ${BLUE}cargo test --test stress_scenarios test_scenario_8_memory_leak_detection --release -- --nocapture --ignored${NC}"
    echo ""
    ((SKIPPED++))
else
    echo -e "${YELLOW}跳过长时间测试 (场景 2, 4, 8)${NC}"
    echo -e "${YELLOW}使用 --all 参数运行所有测试${NC}"
    echo ""
    ((SKIPPED+=3))
fi

# 打印总结
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  测试总结${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "通过: ${GREEN}$PASSED${NC}"
echo -e "失败: ${RED}$FAILED${NC}"
echo -e "跳过: ${YELLOW}$SKIPPED${NC}"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}✗ 部分测试失败${NC}"
    exit 1
else
    echo -e "${GREEN}✓ 所有测试通过!${NC}"
    echo ""
    echo -e "${YELLOW}下一步:${NC}"
    echo -e "  1. 运行基准测试: ${BLUE}./run_stress_tests.sh --bench${NC}"
    echo -e "  2. 运行特定场景: ${BLUE}./run_stress_tests.sh --scenario 3${NC}"
    echo -e "  3. 运行所有测试:  ${BLUE}./run_stress_tests.sh --all${NC}"
    echo ""
fi
