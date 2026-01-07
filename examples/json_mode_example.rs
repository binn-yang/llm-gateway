/// Example: Using JSON mode and structured outputs with the LLM Gateway
///
/// This example demonstrates how to use response_format to get structured
/// JSON outputs from the model. The gateway will handle provider-specific
/// implementations (native support for OpenAI/Gemini, workaround for Anthropic).
///
/// Run with: cargo run --example json_mode_example

use reqwest::Client;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway_url = "http://localhost:8080/v1/chat/completions";
    let api_key = "frontend-app"; // Replace with your gateway API key

    let client = Client::new();

    println!("📋 JSON Mode Example: Structured outputs");
    println!("==========================================\n");

    // Example 1: Basic JSON object mode
    println!("Example 1: JSON object mode (unstructured JSON)...\n");

    let json_object_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": "List three programming languages with their primary use cases. Format as JSON with a 'languages' array."
            }
        ],
        "max_tokens": 500,
        "temperature": 0.7,
        "response_format": {
            "type": "json_object"
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&json_object_request)
        .send()
        .await?;

    // Check for conversion warnings
    if let Some(warnings) = response.headers().get("x-llm-gateway-warnings") {
        println!("⚠️  Warnings: {}\n", warnings.to_str()?);
    }

    let response_body: serde_json::Value = response.json().await?;
    println!("Response:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    let content = response_body["choices"][0]["message"]["content"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(content)?;
    println!("Parsed JSON content:");
    println!("{}\n", serde_json::to_string_pretty(&parsed)?);

    // Example 2: JSON schema mode with strict schema enforcement
    println!("\nExample 2: JSON schema mode with strict schema...\n");

    let schema = json!({
        "name": "user_profile",
        "description": "User profile information",
        "strict": true,
        "schema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Full name"
                },
                "age": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 150
                },
                "email": {
                    "type": "string",
                    "format": "email"
                },
                "skills": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "minItems": 1
                },
                "location": {
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"},
                        "country": {"type": "string"}
                    },
                    "required": ["city", "country"]
                }
            },
            "required": ["name", "age", "email", "skills", "location"],
            "additionalProperties": false
        }
    });

    let schema_request = json!({
        "model": "gemini-1.5-pro", // Gemini has native JSON schema support
        "messages": [
            {
                "role": "user",
                "content": "Create a sample user profile for a software engineer named Alice who lives in Seattle."
            }
        ],
        "max_tokens": 1000,
        "response_format": {
            "type": "json_schema",
            "json_schema": schema
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&schema_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Response with JSON schema:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 3: Combining JSON mode with vision
    println!("\nExample 3: JSON mode with vision (structured image analysis)...\n");

    let vision_json_request = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Analyze this image and provide structured data about detected objects, colors, and text. Format as JSON with arrays for objects, colors, and any detected text."
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
        "max_tokens": 1500,
        "response_format": {
            "type": "json_object"
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&vision_json_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Structured image analysis:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 4: Complex nested schema
    println!("\nExample 4: Complex nested schema (data extraction)...\n");

    let complex_schema = json!({
        "name": "data_extraction",
        "schema": {
            "type": "object",
            "properties": {
                "entities": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "type": {"type": "string", "enum": ["person", "organization", "location", "date"]},
                            "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                            "context": {"type": "string"}
                        },
                        "required": ["name", "type"]
                    }
                },
                "summary": {"type": "string"},
                "sentiment": {
                    "type": "string",
                    "enum": ["positive", "neutral", "negative"]
                }
            },
            "required": ["entities", "summary", "sentiment"]
        }
    });

    let extraction_request = json!({
        "model": "gemini-1.5-pro",
        "messages": [
            {
                "role": "user",
                "content": "Extract structured information from this text: 'Apple Inc. announced record earnings on January 15, 2024. CEO Tim Cook expressed optimism about future growth in Cupertino, California.'"
            }
        ],
        "max_tokens": 1000,
        "response_format": {
            "type": "json_schema",
            "json_schema": complex_schema
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&extraction_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Extracted structured data:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    // Example 5: Error handling with invalid schema
    println!("\nExample 5: Schema validation (demonstrating strict mode)...\n");

    let validation_schema = json!({
        "name": "strict_numbers",
        "strict": true,
        "schema": {
            "type": "object",
            "properties": {
                "numbers": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "minItems": 3,
                    "maxItems": 5
                },
                "sum": {"type": "integer"}
            },
            "required": ["numbers", "sum"]
        }
    });

    let validation_request = json!({
        "model": "gemini-1.5-pro",
        "messages": [
            {
                "role": "user",
                "content": "Generate 4 random integers and calculate their sum. Return in the specified format."
            }
        ],
        "max_tokens": 200,
        "response_format": {
            "type": "json_schema",
            "json_schema": validation_schema
        }
    });

    let response = client
        .post(gateway_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&validation_request)
        .send()
        .await?;

    let response_body: serde_json::Value = response.json().await?;
    println!("Validated structured output:");
    println!("{}\n", serde_json::to_string_pretty(&response_body)?);

    println!("\n✅ JSON mode examples completed!");
    println!("\nKey features demonstrated:");
    println!("  - json_object mode (flexible JSON)");
    println!("  - json_schema mode (strict schema enforcement)");
    println!("  - Complex nested schemas");
    println!("  - Combining JSON mode with vision");
    println!("  - Schema validation and constraints");
    println!("\nProvider differences:");
    println!("  - OpenAI: Native json_object and json_schema support");
    println!("  - Gemini: Native support via response_mime_type and response_schema");
    println!("  - Anthropic: Implemented via system prompt injection (check X-LLM-Gateway-Warnings header)");
    println!("\nNote: Check the X-LLM-Gateway-Warnings header for provider-specific workarounds");

    Ok(())
}
