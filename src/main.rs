use jiaclaw::{providers::ChatMessage, providers::ChatRequest, providers::create_provider, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::default();

    println!("JiaClaw Agent Runtime");
    println!("Provider: {}", config.provider.provider_type);
    println!("Base URL: {}", config.provider.base_url);

    let provider = create_provider(
        &config.provider.provider_type,
        config.provider.base_url.clone(),
        std::env::var("BROKERROUTER_API_KEY").unwrap_or_else(|_| config.provider.api_key.clone()),
    )?;

    let request = ChatRequest {
        model: config
            .provider
            .model
            .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string()),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello! Please introduce yourself briefly.".to_string(),
        }],
        temperature: Some(0.7),
        max_tokens: Some(150),
    };

    println!("\nSending chat completion request...");

    match provider.chat_completion(request).await {
        Ok(response) => {
            println!("\n=== Response ===");
            println!("ID: {}", response.id);
            println!("Model: {}", response.model);
            if let Some(rid) = response.request_id {
                println!("Request ID: {}", rid);
            }
            println!("\nAssistant: {}", response.choices[0].message.content);

            if let Some(usage) = response.usage {
                println!(
                    "\nTokens - Prompt: {}, Completion: {}, Total: {}",
                    usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                );
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
