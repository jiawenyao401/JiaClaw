# JiaClaw

Personal durable agent runtime built on [StateKnot](https://github.com/StateKnot/StateKnot).

## Overview

JiaClaw is a Rust-based agent runtime that provides a robust interface to AI language models through the [Brokerrouter](https://github.com/StateKnot/Brokerrouter) gateway. It offers production-ready features including automatic idempotency, request tracking, and comprehensive error handling.

## Features

- 🚀 **Production-Ready Provider** - Brokerrouter integration with virtual-key authentication
- 🔒 **Idempotency Guarantees** - Automatic generation of unique idempotency keys per request
- 📊 **Request Tracking** - Full request/response tracing with correlation IDs
- 🛡️ **Robust Error Handling** - Type-safe error mapping and detailed diagnostics
- 🧪 **Comprehensive Testing** - Unit and integration tests with HTTP mocking
- 📝 **Well-Documented** - Complete API documentation and configuration examples

## Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- A Brokerrouter API key (see [Getting Access](#getting-access))

### Installation

```bash
git clone https://github.com/jiawenyao401/JiaClaw.git
cd JiaClaw
cargo build --release
```

### Configuration

Create a configuration file or use the provided example:

```bash
cp examples/config.example.toml config.toml
```

Edit `config.toml` to add your Brokerrouter credentials:

```toml
[provider]
type = "brokerrouter"
base_url = "https://api.brokerrouter.dev"
api_key = "brk_live_..."
model = "claude-3-5-sonnet-20241022"
```

Alternatively, set the API key via environment variable:

```bash
export BROKERROUTER_API_KEY=brk_live_...
```

### Running

```bash
cargo run
```

## Architecture

JiaClaw uses a provider-based architecture that supports multiple AI backends. The primary provider is **Brokerrouter**, which offers:

- **Virtual-Key Authentication** - Secure Bearer token authentication
- **Idempotency Keys** - Automatic generation and validation (1-200 printable ASCII characters)
- **Non-Streaming Chat** - OpenAI-compatible `/v1/chat/completions` endpoint
- **Request Tracking** - Response headers include `x-request-id` and `x-brokerrouter-request-id`

### Provider Types

1. **`brokerrouter`** (Recommended) - Production provider for StateKnot's Brokerrouter gateway
2. **`openai_compatible`** (Deprecated) - Temporary escape hatch for development/testing

For production use, always use `provider.type = "brokerrouter"`.

## Current Limitations

Brokerrouter integration is in M2 phase with the following tracked limitations:

- **Streaming** - Not yet supported (tracked in [Brokerrouter#29](https://github.com/StateKnot/Brokerrouter/issues/29))
- **Consumer Guide** - Official docs pending (tracked in [Brokerrouter#28](https://github.com/StateKnot/Brokerrouter/issues/28))
- **Personal Bootstrap** - Onboarding process being defined (tracked in [Brokerrouter#30](https://github.com/StateKnot/Brokerrouter/issues/30))
- **Tool Calling** - Certification pending (tracked in [Brokerrouter#31](https://github.com/StateKnot/Brokerrouter/issues/31))

See [`docs/brokerrouter-gaps.md`](docs/brokerrouter-gaps.md) for detailed status and workarounds.

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_chat_completion_with_mock
```

### Linting

```bash
cargo clippy -- -D warnings
```

### Code Coverage

```bash
cargo tarpaulin --out Html
```

## API Usage Example

```rust
use jiaclaw::{providers::{create_provider, ChatRequest, ChatMessage}, Config};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::default();
    
    let provider = create_provider(
        &config.provider.provider_type,
        config.provider.base_url,
        std::env::var("BROKERROUTER_API_KEY")?,
    )?;
    
    let request = ChatRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            }
        ],
        temperature: Some(0.7),
        max_tokens: Some(150),
    };
    
    let response = provider.chat_completion(request).await?;
    println!("Assistant: {}", response.choices[0].message.content);
    
    Ok(())
}
```

## Getting Access

Brokerrouter is a private service operated by StateKnot. To obtain API credentials:

1. Contact the StateKnot team
2. Complete the onboarding process
3. Receive your virtual key credentials

Note: Brokerrouter repository (`https://github.com/StateKnot/Brokerrouter`) is private. Access requires StateKnot organization membership. The public API contract is documented in this repository.

## Documentation

- [`docs/brokerrouter-gaps.md`](docs/brokerrouter-gaps.md) - Integration status and limitations
- [`examples/config.toml`](examples/config.toml) - Configuration examples
- API Documentation - Run `cargo doc --open`

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Run linter (`cargo clippy`)
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

Built on [StateKnot](https://github.com/StateKnot/StateKnot) infrastructure and powered by [Brokerrouter](https://github.com/StateKnot/Brokerrouter) gateway.

## References

- [StateKnot Organization](https://github.com/StateKnot)
- [Brokerrouter Issues #28-#31](https://github.com/StateKnot/Brokerrouter/issues)
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)
