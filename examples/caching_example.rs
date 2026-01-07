/// Example: Using prompt caching for cost optimization
///
/// This example demonstrates how to use Anthropic's prompt caching feature
/// through the gateway. The gateway automatically applies caching to large
/// system prompts and tool definitions based on configuration.
///
/// Run with: cargo run --example caching_example

use reqwest::Client;
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway_url = "http://localhost:8080/v1/chat/completions";
    let api_key = "frontend-app"; // Replace with your gateway API key

    let client = Client::new();

    println!("💾 Prompt Caching Example: Reducing costs with auto-caching");
    println!("===========================================================\n");

    println!("Prerequisites:");
    println!("  1. Gateway configured with Anthropic provider");
    println!("  2. Auto-caching enabled in config (default):");
    println!("     [providers.anthropic.cache]");
    println!("     auto_cache_system = true");
    println!("     min_system_tokens = 1024");
    println!("     auto_cache_tools = true\n");

    // Example 1: Large system prompt (will be auto-cached)
    println!("Example 1: Large system prompt with auto-caching...\n");

    // Create a large system prompt (> 1024 tokens to trigger auto-caching)
    let large_system_prompt = r#"
You are an expert software architect with deep knowledge of distributed systems,
microservices, cloud computing, and modern software development practices.

Your expertise includes:
- System design and architecture patterns (microservices, event-driven, CQRS, etc.)
- Cloud platforms (AWS, GCP, Azure) and their services
- Container orchestration (Kubernetes, Docker Swarm)
- Database design (SQL, NoSQL, time-series, graph databases)
- API design (REST, GraphQL, gRPC)
- Message queues and event streaming (Kafka, RabbitMQ, AWS SQS)
- Caching strategies (Redis, Memcached, CDN)
- Security best practices (OAuth, JWT, encryption, zero-trust)
- Observability and monitoring (metrics, logs, traces)
- CI/CD pipelines and DevOps practices
- Performance optimization and scalability
- Cost optimization strategies
- Disaster recovery and high availability

When providing architecture recommendations:
1. Consider scalability, reliability, and maintainability
2. Discuss trade-offs between different approaches
3. Provide specific technology recommendations when appropriate
4. Consider cost implications
5. Address security and compliance requirements
6. Suggest monitoring and observability strategies
7. Think about operational complexity
"#.repeat(3); // Repeat to ensure > 1024 tokens

    println!("System prompt size: {} characters (~{} tokens)",
        large_system_prompt.len(),
        large_system_prompt.len() / 4
    );

    let first_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "system",
                "content": large_system_prompt
            },
            {
                "role": "user",
                "content": "Design a scalable e-commerce platform architecture."
            }
        ],
        "max_tokens": 1500,
        "temperature": 0.7
    });

    // First request - cache creation
    println!("\n📤 First request (cache creation)...");
    let start = Instant::now();
    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&first_request)
        .send()
        .await?;

    let first_duration = start.elapsed();
    let response_body: serde_json::Value = response.json().await?;

    println!("Time: {:?}", first_duration);
    println!("\nUsage (first request - cache creation):");
    if let Some(usage) = response_body.get("usage") {
        println!("{}", serde_json::to_string_pretty(usage)?);
        if let Some(cache_creation) = usage.get("cache_creation_input_tokens") {
            println!("\n✅ Cache created with {} tokens!", cache_creation);
        }
    }

    // Second request with same system prompt - cache hit
    println!("\n📤 Second request (cache hit, same system prompt)...");

    let second_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "system",
                "content": large_system_prompt
            },
            {
                "role": "user",
                "content": "Now design a real-time chat application architecture."
            }
        ],
        "max_tokens": 1500,
        "temperature": 0.7
    });

    let start = Instant::now();
    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&second_request)
        .send()
        .await?;

    let second_duration = start.elapsed();
    let response_body: serde_json::Value = response.json().await?;

    println!("Time: {:?}", second_duration);
    println!("\nUsage (second request - cache hit):");
    if let Some(usage) = response_body.get("usage") {
        println!("{}", serde_json::to_string_pretty(usage)?);
        if let Some(cache_read) = usage.get("cache_read_input_tokens") {
            println!("\n🎯 Cache hit! Read {} cached tokens (90% cost reduction)!", cache_read);
        }
    }

    // Compare costs
    if let (Some(first_usage), Some(second_usage)) = (response_body.get("usage"), response_body.get("usage")) {
        println!("\n💰 Cost comparison:");
        println!("  First request (cache creation): ~100% cost");
        println!("  Second request (cache read): ~10% cost for cached portion");
        println!("  Savings: ~90% on repeated system prompt!");
    }

    // Example 2: Auto-caching with tools
    println!("\n\nExample 2: Auto-caching tool definitions...\n");

    let tools = json!([
        {
            "type": "function",
            "function": {
                "name": "query_database",
                "description": "Query a SQL database",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "SQL query"},
                        "database": {"type": "string", "description": "Database name"}
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "deploy_service",
                "description": "Deploy a microservice to Kubernetes",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "service_name": {"type": "string"},
                        "image": {"type": "string"},
                        "replicas": {"type": "integer"},
                        "namespace": {"type": "string"}
                    },
                    "required": ["service_name", "image"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "monitor_metrics",
                "description": "Query monitoring metrics",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "metric_name": {"type": "string"},
                        "time_range": {"type": "string"},
                        "aggregation": {"type": "string", "enum": ["sum", "avg", "max", "min"]}
                    },
                    "required": ["metric_name", "time_range"]
                }
            }
        }
    ]);

    let tools_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "I need to check CPU metrics for the last hour."
            }
        ],
        "max_tokens": 500,
        "tools": tools
    });

    println!("📤 Request with tool definitions (auto-cached)...");
    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&tools_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("\nUsage:");
    if let Some(usage) = response_body.get("usage") {
        println!("{}", serde_json::to_string_pretty(usage)?);
        if usage.get("cache_creation_input_tokens").is_some() {
            println!("\n✅ Tool definitions cached!");
        }
    }

    // Example 3: Demonstrating cache TTL (5 minutes)
    println!("\n\nExample 3: Cache TTL and expiration...\n");
    println!("Cache lifetime:");
    println!("  - Duration: 5 minutes");
    println!("  - Type: Ephemeral (automatically cleaned up)");
    println!("  - Scope: Per-conversation prefix");
    println!("\nTo maximize cache hits:");
    println!("  1. Keep system prompts consistent across requests");
    println!("  2. Reuse tool definitions in conversations");
    println!("  3. Make follow-up requests within 5 minutes");

    // Example 4: Configuration options
    println!("\n\nExample 4: Configuration options...\n");
    println!("Gateway config.toml settings:");
    println!("```toml");
    println!("[providers.anthropic.cache]");
    println!("auto_cache_system = true         # Auto-cache large system prompts");
    println!("min_system_tokens = 1024          # Minimum tokens to trigger caching");
    println!("auto_cache_tools = true           # Auto-cache tool definitions");
    println!("```");
    println!("\nWhen auto-caching is applied:");
    println!("  ✓ System prompts >= 1024 tokens");
    println!("  ✓ Tool definitions (last tool marked)");
    println!("  ✓ Automatic conversion to blocks format");
    println!("  ✓ cache_control added to last block/tool");

    // Example 5: Cost analysis
    println!("\n\nExample 5: Cost analysis...\n");
    println!("Anthropic Pricing (Claude 3.5 Sonnet):");
    println!("  Input tokens:         $3.00 / MTok");
    println!("  Cache write:          $3.75 / MTok (25% markup)");
    println!("  Cache read:           $0.30 / MTok (90% discount)");
    println!("  Output tokens:        $15.00 / MTok");
    println!("\nExample scenario:");
    println!("  System prompt: 5,000 tokens");
    println!("  User message: 100 tokens");
    println!("  Response: 500 tokens");
    println!("\n  First request (cache creation):");
    println!("    Cache write: 5,000 tokens × $3.75/MTok = $0.01875");
    println!("    Regular input: 100 tokens × $3.00/MTok = $0.00030");
    println!("    Output: 500 tokens × $15.00/MTok = $0.00750");
    println!("    Total: $0.02655");
    println!("\n  Second request (cache hit):");
    println!("    Cache read: 5,000 tokens × $0.30/MTok = $0.00150");
    println!("    Regular input: 100 tokens × $3.00/MTok = $0.00030");
    println!("    Output: 500 tokens × $15.00/MTok = $0.00750");
    println!("    Total: $0.00930");
    println!("\n  Savings: 65% reduction per request!");
    println!("  Break-even: 2 requests");
    println!("  10 requests: $0.02655 + (9 × $0.00930) = $0.11025");
    println!("  Without caching: 10 × $0.02505 = $0.25050");
    println!("  Total savings: 56%");

    println!("\n\n✅ Prompt caching examples completed!");
    println!("\nKey takeaways:");
    println!("  - Auto-caching is enabled by default for Anthropic");
    println!("  - Significant cost savings for repeated system prompts");
    println!("  - 5-minute cache TTL (ephemeral caching)");
    println!("  - Gateway handles cache_control automatically");
    println!("  - Monitor cache_creation_input_tokens and cache_read_input_tokens");

    Ok(())
}
