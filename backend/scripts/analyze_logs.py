#!/usr/bin/env python3
import json
import sys
from collections import defaultdict

def analyze_logs(log_file):
    """分析请求日志文件"""

    conversations = defaultdict(lambda: {'request': None, 'response': None})

    with open(log_file, 'r') as f:
        for line in f:
            try:
                log = json.loads(line.strip())

                # 只处理请求和响应事件
                if 'event_type' not in log.get('fields', {}):
                    continue

                event_type = log['fields']['event_type']
                if event_type not in ['request_body', 'response_body']:
                    continue

                request_id = log.get('span', {}).get('request_id')
                if not request_id:
                    continue

                if event_type == 'request_body':
                    body = json.loads(log['fields']['body'])
                    conversations[request_id]['request'] = body
                    conversations[request_id]['model'] = log['span'].get('model')
                    conversations[request_id]['api_key'] = log['span'].get('api_key_name')

                elif event_type == 'response_body':
                    body = json.loads(log['fields']['body'])
                    conversations[request_id]['response'] = body

            except (json.JSONDecodeError, KeyError) as e:
                continue

    # 输出分析结果
    print(f"找到 {len(conversations)} 个对话\n")
    print("=" * 100)

    for idx, (request_id, data) in enumerate(list(conversations.items())[:5], 1):
        print(f"\n对话 #{idx} (Request ID: {request_id[:8]}...)")
        print("-" * 100)

        if data['request']:
            req = data['request']
            print(f"模型: {data.get('model', 'N/A')}")
            print(f"API Key: {data.get('api_key', 'N/A')}")

            # 系统提示
            if 'system' in req and req['system']:
                print(f"\n【系统提示】:")
                for sys_msg in req['system'][:3]:  # 只显示前3个
                    if isinstance(sys_msg, dict) and 'text' in sys_msg:
                        text = sys_msg['text']
                        if len(text) > 200:
                            text = text[:200] + "..."
                        print(f"  - {text}")

            # 用户消息
            if 'messages' in req and req['messages']:
                print(f"\n【用户消息】:")
                for msg in req['messages'][:2]:  # 只显示前2个
                    if 'content' in msg:
                        if isinstance(msg['content'], str):
                            text = msg['content']
                        elif isinstance(msg['content'], list) and len(msg['content']) > 0:
                            content_item = msg['content'][0]
                            text = content_item.get('text', str(content_item))
                        else:
                            text = str(msg['content'])

                        if len(text) > 300:
                            text = text[:300] + "..."
                        print(f"  Role: {msg.get('role', 'unknown')}")
                        print(f"  Content: {text}")

            # 工具配置
            if 'tools' in req and req['tools']:
                print(f"\n【工具数量】: {len(req['tools'])} 个工具")
                if len(req['tools']) <= 3:
                    for tool in req['tools']:
                        print(f"  - {tool.get('name', 'unknown')}")

        if data['response']:
            resp = data['response']
            print(f"\n【LLM响应】:")

            # 响应内容
            if 'content' in resp and resp['content']:
                for content in resp['content'][:1]:  # 只显示第一个内容块
                    if isinstance(content, dict):
                        text = content.get('text', str(content))
                        if len(text) > 400:
                            text = text[:400] + "..."
                        print(f"  {text}")

            # Token使用
            if 'usage' in resp:
                usage = resp['usage']
                print(f"\n【Token使用】:")
                print(f"  输入: {usage.get('input_tokens', 0)}")
                print(f"  输出: {usage.get('output_tokens', 0)}")
                if usage.get('cache_read_input_tokens'):
                    print(f"  缓存读取: {usage.get('cache_read_input_tokens', 0)}")
                if usage.get('cache_creation_input_tokens'):
                    print(f"  缓存创建: {usage.get('cache_creation_input_tokens', 0)}")

        print("\n" + "=" * 100)

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python analyze_logs.py <log_file>")
        sys.exit(1)

    analyze_logs(sys.argv[1])
