#!/bin/bash

# Test script for body logging feature

echo "=== Testing Body Logging Feature ==="
echo ""

# Test 1: Non-streaming request
echo "Test 1: Non-streaming request to /v1/messages"
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer sk-gateway-test-key-001" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "messages": [{"role": "user", "content": "Hello, this is a test message"}],
    "max_tokens": 50
  }' 2>/dev/null | jq .

echo ""
echo "Checking logs for request_body and response_body events..."
sleep 2

# Get today's log file
LOG_FILE="logs/requests.$(date +%Y-%m-%d)"

if [ -f "$LOG_FILE" ]; then
    echo ""
    echo "=== Request Body Event ==="
    grep "request_body" "$LOG_FILE" | tail -1 | jq '.fields'

    echo ""
    echo "=== Response Body Event ==="
    grep "response_body" "$LOG_FILE" | tail -1 | jq '.fields'

    echo ""
    echo "=== Routing Trace Span ==="
    grep "route_model" "$LOG_FILE" | tail -1 | jq '.fields'
else
    echo "Log file not found: $LOG_FILE"
fi

echo ""
echo "=== Test Complete ==="
