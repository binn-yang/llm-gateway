#!/usr/bin/env python3
"""分析LLM Gateway完整对话（请求+响应）"""
import json
import sys
import re
from collections import defaultdict

def parse_sse_response(sse_text):
    """解析Server-Sent Events格式的响应"""
    events = []
    full_text = ""
    usage = {}

    # 按 "event:" 分割SSE流
    parts = sse_text.split('event:')

    for part in parts:
        if not part.strip():
            continue

        lines = part.strip().split('\n', 1)
        if len(lines) < 2:
            continue

        event_type = lines[0].strip()
        data_part = lines[1]

        if not data_part.startswith('data:'):
            continue

        # 提取JSON（去掉"data:"前缀，找到完整的JSON对象）
        json_str = data_part.replace('data:', '', 1).strip()

        # 找到完整的JSON对象（处理嵌套大括号）
        brace_count = 0
        json_end = 0
        for i, char in enumerate(json_str):
            if char == '{':
                brace_count += 1
            elif char == '}':
                brace_count -= 1
                if brace_count == 0:
                    json_end = i + 1
                    break

        if json_end > 0:
            try:
                data = json.loads(json_str[:json_end])
                events.append({'type': event_type, 'data': data})

                # 提取文本内容
                if event_type == 'content_block_delta':
                    text = data.get('delta', {}).get('text', '')
                    full_text += text

                # 提取usage信息
                if event_type == 'message_delta':
                    usage = data.get('usage', {})
                elif event_type == 'message_start':
                    msg_usage = data.get('message', {}).get('usage', {})
                    if msg_usage:
                        usage.update(msg_usage)
            except:
                pass

    return full_text, usage, events

def analyze_conversations(log_file):
    """分析完整对话"""

    conversations = defaultdict(lambda: {'request': None, 'response': None, 'timestamp': None})

    print("=" * 100)
    print("🔍 LLM Gateway 对话分析 - 请求与响应详情")
    print("=" * 100)

    # 第一遍：收集所有数据
    with open(log_file, 'r') as f:
        for line in f:
            try:
                log = json.loads(line.strip())

                if 'fields' not in log:
                    continue

                event_type = log['fields'].get('event_type')
                if not event_type:
                    continue

                request_id = log.get('span', {}).get('request_id')
                if not request_id:
                    continue

                if event_type == 'request_body':
                    try:
                        body = json.loads(log['fields']['body'])
                        conversations[request_id]['request'] = body
                        conversations[request_id]['model'] = log['span'].get('model')
                        conversations[request_id]['api_key'] = log['span'].get('api_key_name')
                        conversations[request_id]['timestamp'] = log.get('timestamp')
                    except:
                        pass

                elif event_type == 'response_body':
                    try:
                        body_text = log['fields']['body']
                        streaming = log['fields'].get('streaming', False)

                        conversations[request_id]['response_raw'] = body_text
                        conversations[request_id]['streaming'] = streaming

                        # 解析响应
                        if streaming:
                            # SSE格式
                            full_text, usage, events = parse_sse_response(body_text)
                            conversations[request_id]['response_text'] = full_text
                            conversations[request_id]['usage'] = usage
                        else:
                            # 普通JSON格式
                            resp_json = json.loads(body_text)
                            text = ""
                            if 'content' in resp_json and resp_json['content']:
                                for content in resp_json['content']:
                                    if isinstance(content, dict) and 'text' in content:
                                        text += content['text']
                            conversations[request_id]['response_text'] = text
                            conversations[request_id]['usage'] = resp_json.get('usage', {})
                    except Exception as e:
                        conversations[request_id]['parse_error'] = str(e)
                        pass

            except json.JSONDecodeError:
                continue

    # 第二遍：输出配对的对话
    complete_conversations = {k: v for k, v in conversations.items()
                             if v['request'] is not None and 'response_text' in v}

    print(f"\n找到 {len(complete_conversations)} 个完整对话（包含请求和响应）\n")

    # 输出前5个对话详情
    for idx, (request_id, conv) in enumerate(list(complete_conversations.items())[:5], 1):
        print(f"\n{'='*100}")
        print(f"对话 #{idx}")
        print(f"{'='*100}")
        print(f"Request ID: {request_id}")
        print(f"时间: {conv.get('timestamp', 'N/A')}")
        print(f"模型: {conv.get('model', 'N/A')}")
        print(f"API Key: {conv.get('api_key', 'N/A')}")
        print(f"流式响应: {'是' if conv.get('streaming') else '否'}")

        req = conv['request']

        # 1. 系统提示词
        print(f"\n{'─'*100}")
        print("📋 系统提示词 (System Prompt)")
        print(f"{'─'*100}")
        if 'system' in req and req['system']:
            for i, sys_msg in enumerate(req['system'], 1):
                if isinstance(sys_msg, dict) and 'text' in sys_msg:
                    text = sys_msg['text']
                    cache = sys_msg.get('cache_control', {}).get('type', '')
                    cache_mark = f" [🔵 Cached: {cache}]" if cache else ""

                    print(f"\n系统提示 #{i}{cache_mark}:")
                    if len(text) > 500:
                        print(f"{text[:500]}\n... (truncated, 总长度: {len(text)} 字符)")
                    else:
                        print(text)
        else:
            print("(无系统提示)")

        # 2. 用户输入
        print(f"\n{'─'*100}")
        print("💬 用户输入 (User Messages)")
        print(f"{'─'*100}")
        if 'messages' in req and req['messages']:
            for msg_idx, msg in enumerate(req['messages'], 1):
                role = msg.get('role', 'unknown')

                if 'content' in msg:
                    if isinstance(msg['content'], str):
                        text = msg['content']
                    elif isinstance(msg['content'], list) and len(msg['content']) > 0:
                        content_item = msg['content'][0]
                        text = content_item.get('text', str(content_item))

                        # 检查缓存
                        cache = content_item.get('cache_control', {}).get('type', '')
                        if cache:
                            text += f" [🔵 Cached: {cache}]"
                    else:
                        text = str(msg['content'])

                    print(f"\n消息 #{msg_idx} [{role}]:")
                    if len(text) > 600:
                        print(f"{text[:600]}\n... (truncated, 总长度: {len(text)} 字符)")
                    else:
                        print(text)

        # 3. 请求配置
        print(f"\n{'─'*100}")
        print("⚙️  请求配置")
        print(f"{'─'*100}")
        print(f"max_tokens: {req.get('max_tokens', 'N/A')}")
        print(f"temperature: {req.get('temperature', 'default')}")
        print(f"stream: {req.get('stream', False)}")

        if 'tools' in req and req['tools']:
            print(f"工具数量: {len(req['tools'])}")
            if len(req['tools']) <= 10:
                print("工具列表:")
                for tool in req['tools'][:10]:
                    tool_name = tool.get('name', 'unknown')
                    if tool_name.startswith('mcp__'):
                        parts = tool_name.split('__')
                        if len(parts) >= 3:
                            tool_name = f"{parts[1]}/{parts[2]}"
                    desc = tool.get('description', '')
                    if len(desc) > 60:
                        desc = desc[:60] + "..."
                    print(f"  • {tool_name}: {desc}")

        if 'output_config' in req:
            output_fmt = req['output_config'].get('format', {}).get('type', 'text')
            print(f"输出格式: {output_fmt}")
            if output_fmt == 'json_schema':
                schema = req['output_config']['format'].get('schema', {})
                if 'properties' in schema:
                    print(f"  Schema字段: {', '.join(schema['properties'].keys())}")

        # 4. LLM响应
        print(f"\n{'─'*100}")
        print("🤖 LLM 响应 (Assistant Response)")
        print(f"{'─'*100}")
        response_text = conv.get('response_text', '')
        if response_text:
            if len(response_text) > 800:
                print(f"{response_text[:800]}\n... (truncated, 总长度: {len(response_text)} 字符)")
            else:
                print(response_text)
        else:
            print("(无响应内容或解析失败)")

        # 5. Token使用情况
        if 'usage' in conv and conv['usage']:
            usage = conv['usage']
            print(f"\n{'─'*100}")
            print("📊 Token 使用统计")
            print(f"{'─'*100}")
            print(f"输入 tokens: {usage.get('input_tokens', 0)}")
            print(f"输出 tokens: {usage.get('output_tokens', 0)}")

            if usage.get('cache_creation_input_tokens'):
                print(f"缓存创建 tokens: {usage.get('cache_creation_input_tokens', 0)} (成本 +25%)")
            if usage.get('cache_read_input_tokens'):
                print(f"缓存读取 tokens: {usage.get('cache_read_input_tokens', 0)} (成本 -90%)")

            total = usage.get('input_tokens', 0) + usage.get('output_tokens', 0)
            print(f"总计: {total} tokens")

    # 总结统计
    print(f"\n{'='*100}")
    print("📈 对话统计摘要")
    print(f"{'='*100}")

    total_input = sum(conv.get('usage', {}).get('input_tokens', 0) for conv in complete_conversations.values())
    total_output = sum(conv.get('usage', {}).get('output_tokens', 0) for conv in complete_conversations.values())
    total_cache_read = sum(conv.get('usage', {}).get('cache_read_input_tokens', 0) for conv in complete_conversations.values())
    total_cache_create = sum(conv.get('usage', {}).get('cache_creation_input_tokens', 0) for conv in complete_conversations.values())

    print(f"总对话数: {len(complete_conversations)}")
    print(f"总输入 tokens: {total_input:,}")
    print(f"总输出 tokens: {total_output:,}")
    print(f"缓存读取 tokens: {total_cache_read:,}")
    print(f"缓存创建 tokens: {total_cache_create:,}")
    print(f"总计: {total_input + total_output:,} tokens")

    # 缓存效率
    if total_cache_read > 0:
        cache_ratio = (total_cache_read / (total_input or 1)) * 100
        print(f"\n缓存命中率: {cache_ratio:.1f}%")
        print(f"通过缓存节省的成本约: {total_cache_read * 0.9:,.0f} tokens 等效成本")

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python analyze_conversations.py <log_file>")
        sys.exit(1)

    analyze_conversations(sys.argv[1])
