# http_server
Receive data via HTTP POST requests and forward to the pipeline.

!!! note "Feature Flag Required"
    This input requires the `http_server` feature to be enabled when building fiddler.
    ```bash
    cargo build --features http_server
    ```

=== "Basic"
    ```yml
    input:
      http_server:
        port: 8080
    ```

=== "Full"
    ```yml
    input:
      http_server:
        address: "0.0.0.0"
        port: 8080
        path: "/ingest"
        max_body_size: 10485760
        acknowledgment: true
        cors_enabled: false
    ```

## Endpoints

### POST `{path}` (default: `/ingest`)
Accepts JSON data and feeds it to the pipeline.

**Request Format:**

- Content-Type: `application/json`
- Body: JSON object or array of JSON objects

**Response Codes:**

| Status | Description | Response Body |
|--------|-------------|---------------|
| 200 | Messages accepted/processed | `{"accepted": 1, "processed": 1}` |
| 400 | Invalid JSON | `{"error": "Invalid JSON", "details": "<message>"}` |
| 413 | Payload too large | `{"error": "Payload too large", "max_size": 10485760}` |
| 500 | All messages failed | `{"error": "All messages failed", "errors": [...]}` |

### GET `/health`
Health check endpoint.

**Response:**
```json
{"status": "healthy"}
```

## Usage Example

Send a single event:
```bash
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -d '{"event": "click", "user_id": 123}'
```

Send multiple events:
```bash
curl -X POST http://localhost:8080/ingest \
  -H "Content-Type: application/json" \
  -d '[{"event": "click"}, {"event": "view"}]'
```

## Fields

### `address`
Network address to bind the HTTP server.
Type: `string`
Default: `"0.0.0.0"`
Required: `false`

### `port`
Port number to listen on.
Type: `integer`
Default: `8080`
Required: `false`

### `path`
HTTP endpoint path for ingestion. Must start with `/`.
Type: `string`
Default: `"/ingest"`
Required: `false`

### `max_body_size`
Maximum request body size in bytes. Requests exceeding this limit receive a 413 error.
Type: `integer`
Default: `10485760` (10 MB)
Required: `false`

### `acknowledgment`
When `true`, the HTTP response is delayed until the message has been fully processed through the pipeline (including all outputs). When `false`, the response is sent immediately after the message is queued.
Type: `boolean`
Default: `true`
Required: `false`

### `cors_enabled`
Enable CORS headers for cross-origin requests from browsers.
Type: `boolean`
Default: `false`
Required: `false`

## Message Format

Each JSON object in the request body becomes a message with:

- `bytes`: The JSON object serialized as bytes
- `metadata`: Empty (can be populated by processors)
- `message_type`: `Default`
- `stream_id`: `None`

## Acknowledgment Modes

### With Acknowledgment (default)
```yml
input:
  http_server:
    acknowledgment: true
```

- HTTP response waits for pipeline completion
- Response includes processing status
- Enables at-least-once delivery semantics
- Higher latency but guaranteed feedback

### Without Acknowledgment
```yml
input:
  http_server:
    acknowledgment: false
```

- HTTP response sent immediately after queueing
- Fire-and-forget mode
- Lower latency but no processing feedback
- Useful for high-throughput scenarios where delivery guarantees are not critical

## Full Pipeline Example

```yml
label: HTTP Ingestion Pipeline
input:
  http_server:
    address: "0.0.0.0"
    port: 8080
    path: "/events"
    max_body_size: 5242880  # 5 MB
    acknowledgment: true
    cors_enabled: true
processors:
  - fiddlerscript:
      source: |
        root.timestamp = timestamp()
        root.processed = true
output:
  stdout: {}
```
