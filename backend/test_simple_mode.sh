#!/bin/bash

# 测试简化日志模式的脚本

set -e

echo "=== 测试简化日志模式 ==="
echo ""

# 检查当前日志文件
LOG_FILE="backend/logs/requests.$(date +%Y-%m-%d)"

if [ ! -f "$LOG_FILE" ]; then
    echo "警告: 日志文件 $LOG_FILE 不存在"
    echo "请先启动服务器: cargo run --release -- --config /tmp/test_simple_logging.toml start"
    exit 1
fi

echo "日志文件: $LOG_FILE"
echo ""

# 测试1: 检查简化请求日志
echo "测试1: 检查简化请求日志 (simple_request)"
echo "-------------------------------------------"
SIMPLE_REQUESTS=$(grep "simple_request" "$LOG_FILE" 2>/dev/null | tail -3)
if [ -n "$SIMPLE_REQUESTS" ]; then
    echo "$SIMPLE_REQUESTS" | jq -c '.fields | {event_type, body: .body[:100], body_size}'
    echo ""
else
    echo "未找到 simple_request 事件"
fi
echo ""

# 测试2: 检查简化响应日志
echo "测试2: 检查简化响应日志 (simple_response)"
echo "-------------------------------------------"
SIMPLE_RESPONSES=$(grep "simple_response" "$LOG_FILE" 2>/dev/null | tail -3)
if [ -n "$SIMPLE_RESPONSES" ]; then
    echo "$SIMPLE_RESPONSES" | jq -c '.fields | {event_type, body: .body[:100], streaming}'
    echo ""
else
    echo "未找到 simple_response 事件"
fi
echo ""

# 测试3: 验证不应该包含系统提示词
echo "测试3: 验证简化模式不包含系统提示词"
echo "-------------------------------------------"
SYSTEM_IN_SIMPLE=$(grep "simple_request" "$LOG_FILE" 2>/dev/null | grep "system" || true)
if [ -z "$SYSTEM_IN_SIMPLE" ]; then
    echo "✅ 通过: 简化模式不包含系统提示词"
else
    echo "❌ 失败: 简化模式包含了系统提示词"
fi
echo ""

# 测试4: 验证不应该包含工具定义
echo "测试4: 验证简化模式不包含工具定义"
echo "-------------------------------------------"
TOOLS_IN_SIMPLE=$(grep "simple_request" "$LOG_FILE" 2>/dev/null | grep "tools" || true)
if [ -z "$TOOLS_IN_SIMPLE" ]; then
    echo "✅ 通过: 简化模式不包含工具定义"
else
    echo "❌ 失败: 简化模式包含了工具定义"
fi
echo ""

# 测试5: 比较日志大小
echo "测试5: 比较日志大小"
echo "-------------------------------------------"
SIMPLE_SIZE=$(grep "simple_request" "$LOG_FILE" 2>/dev/null | wc -c || echo 0)
FULL_SIZE=$(grep "request_body" "$LOG_FILE" 2>/dev/null | wc -c || echo 0)

echo "简化模式总大小: $SIMPLE_SIZE bytes"
echo "完整模式总大小: $FULL_SIZE bytes"

if [ "$SIMPLE_SIZE" -gt 0 ] && [ "$FULL_SIZE" -gt 0 ]; then
    REDUCTION=$(awk "BEGIN {printf \"%.1f\", (1 - $SIMPLE_SIZE/$FULL_SIZE) * 100}")
    echo "大小减少: ${REDUCTION}%"
elif [ "$SIMPLE_SIZE" -gt 0 ]; then
    echo "✅ 简化模式日志已记录"
fi
echo ""

# 测试6: 显示示例日志
echo "测试6: 显示简化模式日志示例"
echo "-------------------------------------------"
echo "请求示例:"
grep "simple_request" "$LOG_FILE" 2>/dev/null | tail -1 | jq '.fields.body' || echo "无数据"
echo ""
echo "响应示例:"
grep "simple_response" "$LOG_FILE" 2>/dev/null | tail -1 | jq '.fields.body' || echo "无数据"
echo ""

echo "=== 测试完成 ==="
echo ""
echo "提示: 如果没有看到日志数据,请执行以下步骤:"
echo "1. 启动服务器: cargo run --release -- --config /tmp/test_simple_logging.toml start"
echo "2. 发送测试请求 (见下面的curl命令)"
echo "3. 重新运行此脚本"
echo ""
echo "测试请求示例:"
echo 'curl -X POST http://localhost:8080/v1/messages \'
echo '  -H "Authorization: Bearer test-key-12345" \'
echo '  -H "Content-Type: application/json" \'
echo '  -d '"'"'{'
echo '    "model": "claude-3-5-sonnet-20241022",'
echo '    "messages": [{"role": "user", "content": "What is 2+2?"}],'
echo '    "max_tokens": 100,'
echo '    "system": "You are a math tutor.",'
echo '    "tools": [{"name": "calculator", "description": "Calculate"}]'
echo '  }'"'"
