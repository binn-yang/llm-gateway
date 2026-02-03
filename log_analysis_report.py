#!/usr/bin/env python3
"""分析LLM Gateway日志文件"""
import json
import sys
from collections import defaultdict, Counter
from datetime import datetime

def analyze_detailed_logs(log_file):
    """详细分析日志文件"""

    request_bodies = []
    conversations = defaultdict(dict)
    errors = []
    stats = {
        'total_requests': 0,
        'models': Counter(),
        'api_keys': Counter(),
        'errors': Counter(),
    }

    print("=" * 100)
    print("LLM Gateway 日志分析报告")
    print("=" * 100)

    with open(log_file, 'r') as f:
        for line in f:
            try:
                log = json.loads(line.strip())

                # 统计错误
                if log.get('level') == 'ERROR':
                    error_msg = log.get('fields', {}).get('message', '')
                    errors.append({
                        'time': log.get('timestamp'),
                        'message': error_msg,
                        'target': log.get('target')
                    })
                    stats['errors'][error_msg] += 1

                # 处理请求body事件
                if log.get('fields', {}).get('event_type') == 'request_body':
                    stats['total_requests'] += 1
                    request_id = log.get('span', {}).get('request_id')

                    try:
                        body = json.loads(log['fields']['body'])
                        model = body.get('model', 'unknown')
                        stats['models'][model] += 1

                        api_key = log.get('span', {}).get('api_key_name', 'unknown')
                        stats['api_keys'][api_key] += 1

                        conversations[request_id] = {
                            'model': model,
                            'api_key': api_key,
                            'request': body,
                            'timestamp': log.get('timestamp')
                        }

                        request_bodies.append({
                            'request_id': request_id,
                            'model': model,
                            'body': body
                        })
                    except:
                        pass

            except json.JSONDecodeError:
                continue

    # 输出统计信息
    print(f"\n📊 统计概览")
    print("-" * 100)
    print(f"总请求数: {stats['total_requests']}")
    print(f"错误数: {sum(stats['errors'].values())}")
    print(f"\n使用的模型:")
    for model, count in stats['models'].most_common():
        print(f"  - {model}: {count} 次")
    print(f"\nAPI Keys:")
    for key, count in stats['api_keys'].most_common():
        print(f"  - {key}: {count} 次")

    # 输出错误信息
    if errors:
        print(f"\n⚠️  错误列表 (最近10个)")
        print("-" * 100)
        for error in errors[-10:]:
            print(f"时间: {error['time']}")
            print(f"消息: {error['message']}")
            print(f"来源: {error['target']}")
            print()

    # 输出前5个对话详情
    print(f"\n💬 对话详情 (前5个)")
    print("=" * 100)

    for idx, (request_id, data) in enumerate(list(conversations.items())[:5], 1):
        print(f"\n【对话 #{idx}】")
        print(f"Request ID: {request_id}")
        print(f"时间: {data.get('timestamp', 'N/A')}")
        print(f"模型: {data.get('model')}")
        print(f"API Key: {data.get('api_key')}")
        print("-" * 100)

        req = data['request']

        # 系统提示
        if 'system' in req and req['system']:
            print(f"\n🔧 系统提示词:")
            for i, sys_msg in enumerate(req['system'][:3], 1):
                if isinstance(sys_msg, dict) and 'text' in sys_msg:
                    text = sys_msg['text']
                    # 检查是否有cache_control
                    cache = sys_msg.get('cache_control', {}).get('type', '')
                    cache_mark = " [cached]" if cache else ""

                    if len(text) > 150:
                        text = text[:150] + "..."
                    print(f"  {i}. {text}{cache_mark}")

        # 用户消息
        if 'messages' in req and req['messages']:
            print(f"\n👤 用户输入:")
            for msg in req['messages'][:2]:
                role = msg.get('role', 'unknown')

                if 'content' in msg:
                    if isinstance(msg['content'], str):
                        text = msg['content']
                    elif isinstance(msg['content'], list) and len(msg['content']) > 0:
                        content_item = msg['content'][0]
                        text = content_item.get('text', str(content_item)[:100])

                        # 检查是否有cache_control
                        cache = content_item.get('cache_control', {}).get('type', '')
                        if cache:
                            text += f" [cached: {cache}]"
                    else:
                        text = str(msg['content'])[:100]

                    if len(text) > 300:
                        text = text[:300] + "..."

                    print(f"  [{role}] {text}")

        # 配置参数
        print(f"\n⚙️  请求参数:")
        print(f"  - max_tokens: {req.get('max_tokens', 'N/A')}")
        print(f"  - stream: {req.get('stream', False)}")
        print(f"  - temperature: {req.get('temperature', 'default')}")

        # 工具数量
        if 'tools' in req and req['tools']:
            print(f"  - 工具数量: {len(req['tools'])}")
            if len(req['tools']) <= 5:
                print(f"  - 工具列表:")
                for tool in req['tools'][:5]:
                    tool_name = tool.get('name', 'unknown')
                    # 简化MCP工具名称
                    if tool_name.startswith('mcp__'):
                        tool_name = tool_name.split('__')[-1]
                    print(f"      * {tool_name}")

        # 输出配置
        if 'output_config' in req:
            output_cfg = req['output_config']
            print(f"  - 输出格式: {output_cfg.get('format', {}).get('type', 'text')}")

        print()

    # 分析客户端交互模式
    print("\n" + "=" * 100)
    print("📝 客户端交互模式分析")
    print("=" * 100)

    # 统计各种交互类型
    interaction_types = Counter()
    has_tools = 0
    has_system_prompt = 0
    has_cache = 0
    streaming_count = 0

    for conv in conversations.values():
        req = conv['request']

        if req.get('stream'):
            streaming_count += 1

        if req.get('tools'):
            has_tools += 1
            interaction_types['工具调用'] += 1

        if req.get('system'):
            has_system_prompt += 1

        # 检查缓存使用
        if req.get('system'):
            for sys_msg in req['system']:
                if isinstance(sys_msg, dict) and sys_msg.get('cache_control'):
                    has_cache += 1
                    interaction_types['使用缓存'] += 1
                    break

        # 检查输出格式
        if 'output_config' in req:
            fmt = req['output_config'].get('format', {}).get('type')
            if fmt == 'json_schema':
                interaction_types['结构化输出(JSON Schema)'] += 1

    print(f"\n交互特征:")
    print(f"  - 使用流式响应: {streaming_count}/{len(conversations)} ({streaming_count/len(conversations)*100:.1f}%)")
    print(f"  - 包含工具调用: {has_tools}/{len(conversations)} ({has_tools/len(conversations)*100:.1f}%)")
    print(f"  - 包含系统提示: {has_system_prompt}/{len(conversations)} ({has_system_prompt/len(conversations)*100:.1f}%)")
    print(f"  - 使用prompt缓存: {has_cache}/{len(conversations)} ({has_cache/len(conversations)*100:.1f}%)")

    print(f"\n交互类型分布:")
    for itype, count in interaction_types.most_common():
        print(f"  - {itype}: {count} 次")

    print("\n" + "=" * 100)

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python log_analysis_report.py <log_file>")
        sys.exit(1)

    analyze_detailed_logs(sys.argv[1])
