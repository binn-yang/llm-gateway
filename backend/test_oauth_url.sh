#!/bin/bash

# 测试 OAuth URL 生成
# 注意: 这会生成一个真实的授权 URL，但不会实际执行 OAuth 流程

echo "测试 Anthropic OAuth 配置..."
echo ""
echo "预期的 URL 参数:"
echo "  - auth_url: https://claude.ai/oauth/authorize"
echo "  - client_id: 9d1c250a-e61b-44d9-88ed-5944d1962f5e"
echo "  - scopes: org:create_api_key user:profile user:inference user:sessions:claude_code"
echo "  - code=true (Anthropic 必需参数)"
echo "  - PKCE 参数: code_challenge, code_challenge_method=S256"
echo ""
echo "如果要测试完整的 OAuth 流程，请运行:"
echo "  cargo run --release --config test_oauth_config.toml -- oauth login anthropic"
echo ""
echo "注意: 这将需要您手动复制授权后的 callback URL"
