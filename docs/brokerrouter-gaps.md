# Brokerrouter Integration Status

This document tracks the integration between JiaClaw and [Brokerrouter](https://github.com/StateKnot/Brokerrouter), a private Rust AI gateway developed by StateKnot.

## Current Implementation (M2)

JiaClaw's `BrokerrouterProvider` implements the following production-ready features:

### Implemented
- ✅ **Non-streaming Chat Completions** - `POST /v1/chat/completions` with `stream: false`
- ✅ **Virtual-Key Authentication** - Bearer token authentication via `Authorization` header
- ✅ **Idempotency Keys** - Automatic generation of unique `Idempotency-Key` per request (1-200 printable ASCII characters)
- ✅ **Request Tracking** - Captures `x-request-id` and `x-brokerrouter-request-id` from response headers
- ✅ **Error Mapping** - Proper error handling for API failures with status codes and messages
- ✅ **Type-Safe Requests** - Strongly-typed Rust structures for request/response payloads

### API Contract

The provider communicates with Brokerrouter using the following contract:

**Endpoint:** `POST {base_url}/v1/chat/completions`

**Request Headers:**
```
Authorization: Bearer <virtual-key>
Idempotency-Key: <1-200 printable ASCII chars>
Content-Type: application/json
```

**Request Body:**
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {"role": "user", "content": "..."}
  ],
  "temperature": 0.7,
  "max_tokens": 1000,
  "stream": false
}
```

**Response Headers:**
```
x-request-id: <request-id>
x-brokerrouter-request-id: <broker-specific-id>
```

**Response Body:**
```json
{
  "id": "chatcmpl-...",
  "model": "claude-3-5-sonnet-20241022",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "..."
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  }
}
```

## Limitations & Tracked Issues

The following features are tracked in the Brokerrouter repository and not yet supported:

### Streaming Support
- **Issue:** [StateKnot/Brokerrouter#29](https://github.com/StateKnot/Brokerrouter/issues/29)
- **Status:** Currently rejected with error
- **Impact:** JiaClaw cannot use streaming responses yet
- **Workaround:** Use non-streaming mode (implemented)

### Consumer Documentation
- **Issue:** [StateKnot/Brokerrouter#28](https://github.com/StateKnot/Brokerrouter/issues/28)
- **Status:** Consumer guide pending
- **Impact:** Limited official documentation for integration
- **Workaround:** This document and code examples serve as interim guide

### Personal Bootstrap
- **Issue:** [StateKnot/Brokerrouter#30](https://github.com/StateKnot/Brokerrouter/issues/30)
- **Status:** Personal onboarding process being defined
- **Impact:** Setup may require manual coordination
- **Workaround:** Contact StateKnot team for API key provisioning

### Tool Calling Certification
- **Issue:** [StateKnot/Brokerrouter#31](https://github.com/StateKnot/Brokerrouter/issues/31)
- **Status:** Tool roundtrip certification pending
- **Impact:** Function calling features not yet certified
- **Workaround:** Use text-only completions for now

## Architecture Notes

### Why Brokerrouter is Recommended

Brokerrouter is the **primary recommended provider** for JiaClaw production deployments because:

1. **StateKnot Integration** - Built by the same organization that provides JiaClaw's StateKnot foundation
2. **Virtual Key Security** - Proper key rotation and management capabilities
3. **Idempotency Guarantees** - Built-in protection against duplicate requests
4. **Request Tracing** - Full request/response tracking for debugging and auditing
5. **Production Stability** - Rust-based gateway designed for reliability
6. **Cost Control** - Centralized billing and usage tracking

### Alternative Providers

The `openai_compatible` provider remains available as a **temporary escape hatch** for:
- Local development and testing
- Migration scenarios
- Emergency fallback

**Note:** The `openai_compatible` provider is deprecated and will be removed in a future release. All production deployments should use `BrokerrouterProvider`.

## Repository Access Note

Brokerrouter is a **private repository** (`https://github.com/StateKnot/Brokerrouter`). Cloud Agents or users without StateKnot organization access will see 404 errors when attempting to browse the repository. This is expected and **does not indicate the repository is missing**.

Access to Brokerrouter source code is restricted to StateKnot organization members. The API contract documented above is sufficient for integration without direct repository access.

## Testing

JiaClaw includes comprehensive unit tests for the Brokerrouter provider using wiremock for HTTP mocking. Tests verify:

- ✅ Idempotency key generation and format validation
- ✅ Authorization header presence
- ✅ Request body structure
- ✅ Response parsing and request ID extraction
- ✅ Error handling and status code mapping
- ✅ Non-streaming mode enforcement

Run tests with:
```bash
cargo test
```

## Configuration

See `examples/config.toml` for complete configuration examples.

Basic Brokerrouter configuration:

```toml
[provider]
type = "brokerrouter"
base_url = "https://api.brokerrouter.dev"
api_key = "your-virtual-key-here"
model = "claude-3-5-sonnet-20241022"
```

Set the API key via environment variable:
```bash
export BROKERROUTER_API_KEY=your-virtual-key-here
cargo run
```

## Future Roadmap

As Brokerrouter issues #28-#31 are resolved, JiaClaw will be updated to support:

1. Streaming chat completions (when #29 ships)
2. Improved onboarding flow (per #30)
3. Function/tool calling (after #31 certification)
4. Updated documentation (tracking #28)

## Getting Help

- **Brokerrouter Issues:** File at [StateKnot/Brokerrouter](https://github.com/StateKnot/Brokerrouter/issues)
- **JiaClaw Issues:** File at [jiawenyao401/JiaClaw](https://github.com/jiawenyao401/JiaClaw/issues)
- **API Key Access:** Contact StateKnot team directly

## References

- [Brokerrouter Repository](https://github.com/StateKnot/Brokerrouter) (private)
- [StateKnot Organization](https://github.com/StateKnot)
- [JiaClaw Repository](https://github.com/jiawenyao401/JiaClaw)
