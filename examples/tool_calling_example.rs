/// Example: Using tool/function calling with the LLM Gateway
///
/// This example demonstrates how to use tool calling (function calling) through
/// the gateway. The gateway will automatically convert between OpenAI and
/// provider-specific formats (Anthropic, Gemini, etc.).
///
/// Run with: cargo run --example tool_calling_example

use reqwest::Client;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway_url = "http://localhost:8080/v1/chat/completions";
    let api_key = "frontend-app"; // Replace with your gateway API key

    let client = Client::new();

    println!("🔧 Tool Calling Example: Multi-turn conversation with function calls");
    println!("===================================================================\n");

    // Define tools/functions that the model can use
    let tools = json!([
        {
            "type": "function",
            "function": {
                "name": "get_current_weather",
                "description": "Get the current weather in a given location",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city and state, e.g. San Francisco, CA"
                        },
                        "unit": {
                            "type": "string",
                            "enum": ["celsius", "fahrenheit"],
                            "description": "The temperature unit"
                        }
                    },
                    "required": ["location"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_stock_price",
                "description": "Get the current stock price for a given ticker symbol",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": {
                            "type": "string",
                            "description": "Stock ticker symbol, e.g. AAPL"
                        }
                    },
                    "required": ["symbol"]
                }
            }
        }
    ]);

    // Step 1: Initial request with tool definitions
    println!("Step 1: Asking a question that requires tool use...\n");

    let initial_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "What's the weather like in San Francisco and what's Apple's stock price?"
            }
        ],
        "max_tokens": 1024,
        "tools": tools,
        "tool_choice": "auto"
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&initial_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Assistant's response with tool calls:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    // Extract tool calls from the response
    let tool_calls = response_body["choices"][0]["message"]["tool_calls"].clone();
    if tool_calls.is_null() {
        println!("No tool calls made. Response was text only.");
        return Ok(());
    }

    println!("\nStep 2: Simulating tool execution and providing results...\n");

    // Simulate executing the tools and getting results
    // In a real application, you would call your actual functions here
    let simulated_weather_result = json!({
        "location": "San Francisco, CA",
        "temperature": 72,
        "unit": "fahrenheit",
        "conditions": "Sunny with light fog in the morning"
    });

    let simulated_stock_result = json!({
        "symbol": "AAPL",
        "price": 178.45,
        "change": 2.15,
        "change_percent": 1.22
    });

    // Step 3: Send tool results back to continue the conversation
    println!("Step 3: Sending tool results back to the model...\n");

    let followup_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "What's the weather like in San Francisco and what's Apple's stock price?"
            },
            {
                "role": "assistant",
                "content": response_body["choices"][0]["message"]["content"].clone(),
                "tool_calls": tool_calls.clone()
            },
            {
                "role": "tool",
                "name": "get_current_weather",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_call_id": tool_calls[0]["id"].as_str().unwrap(),
                        "content": serde_json::to_string(&simulated_weather_result)?
                    }
                ]
            },
            {
                "role": "tool",
                "name": "get_stock_price",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_call_id": tool_calls[1]["id"].as_str().unwrap(),
                        "content": serde_json::to_string(&simulated_stock_result)?
                    }
                ]
            }
        ],
        "max_tokens": 1024,
        "tools": tools
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&followup_request)
        .send()
        .await?;

    let final_response: serde_json::Value = response.json().await?;
    println!("Final assistant response with tool results:");
    println!("{}\n", serde_json::to_string_pretty(&final_response)?);

    // Example 4: Forcing a specific tool
    println!("\nExample 4: Forcing use of a specific tool...\n");

    let forced_tool_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "I need weather information"
            }
        ],
        "max_tokens": 512,
        "tools": tools,
        "tool_choice": {
            "type": "function",
            "function": {
                "name": "get_current_weather"
            }
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&forced_tool_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Response with forced tool:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 5: Streaming with tool calls
    println!("\nExample 5: Streaming response with tool calls...\n");
    println!("Note: Streaming tool calls show incremental JSON assembly\n");

    let streaming_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "Get the weather for Boston"
            }
        ],
        "max_tokens": 512,
        "tools": tools,
        "stream": true
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&streaming_request)
        .send()
        .await?;

    let body = response.text().await?;
    println!("Streaming chunks (first few):");
    for (i, line) in body.lines().take(10).enumerate() {
        if line.starts_with("data: ") && !line.contains("[DONE]") {
            println!("Chunk {}: {}", i + 1, line);
        }
    }

    println!("\n✅ Tool calling examples completed!");
    println!("\nKey features demonstrated:");
    println!("  - Tool/function definitions");
    println!("  - Auto tool selection");
    println!("  - Forcing specific tool use");
    println!("  - Multi-turn conversations with tool results");
    println!("  - Streaming tool calls");
    println!("\nThe gateway automatically converts between:");
    println!("  - OpenAI tool format ↔ Anthropic tool format");
    println!("  - OpenAI tool format ↔ Gemini function declarations");

    Ok(())
}
