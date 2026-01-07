/// Example: Using vision/image support with the LLM Gateway
///
/// This example demonstrates how to send image requests through the gateway
/// using the OpenAI-compatible API format. The gateway will automatically
/// convert the request to the appropriate format for the target provider
/// (Anthropic, Gemini, etc.).
///
/// Run with: cargo run --example vision_example

use reqwest::Client;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Gateway endpoint (adjust if your gateway runs on a different port)
    let gateway_url = "http://localhost:8080/v1/chat/completions";

    // Your API key configured in the gateway
    let api_key = "frontend-app"; // Replace with your actual gateway API key

    let client = Client::new();

    println!("🖼️  Vision Example: Analyzing an image");
    println!("=========================================\n");

    // Example 1: Using a base64-encoded image (data URL)
    println!("Example 1: Analyzing a base64-encoded image...\n");

    let request_body = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "What do you see in this image? Describe it in detail."
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAv/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCwAAAAA//Z",
                            "detail": "high"
                        }
                    }
                ]
            }
        ],
        "max_tokens": 1024,
        "temperature": 0.7
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let status = response.status();
    let response_body: serde_json::Value = response.json().await?;

    println!("Status: {}", status);
    println!("Response: {}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 2: Multiple images in a single request
    println!("\nExample 2: Comparing multiple images...\n");

    let multi_image_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Compare these two images and describe their differences:"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD...",
                            "detail": "low"
                        }
                    },
                    {
                        "type": "text",
                        "text": "versus"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA...",
                            "detail": "high"
                        }
                    }
                ]
            }
        ],
        "max_tokens": 2048
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&multi_image_request)
        .send()
        .await?;

    let status = response.status();
    let response_body: serde_json::Value = response.json().await?;

    println!("Status: {}", status);
    println!("Response: {}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 3: Image with structured output (combining vision + JSON mode)
    println!("\nExample 3: Image analysis with structured JSON output...\n");

    let structured_vision_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Analyze this image and extract: objects, colors, and overall scene description."
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD..."
                        }
                    }
                ]
            }
        ],
        "max_tokens": 1024,
        "response_format": {
            "type": "json_object"
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&structured_vision_request)
        .send()
        .await?;

    let status = response.status();
    let response_body: serde_json::Value = response.json().await?;

    println!("Status: {}", status);
    println!("Response: {}\n", serde_json::to_string_pretty(&response_body)?);

    println!("\n✅ Vision examples completed!");
    println!("\nKey features demonstrated:");
    println!("  - Base64-encoded images (data URLs)");
    println!("  - Multiple images in a single request");
    println!("  - Image detail levels (low/high)");
    println!("  - Combining vision with structured outputs");
    println!("\nNote: Replace the base64 placeholders with actual image data");

    Ok(())
}
