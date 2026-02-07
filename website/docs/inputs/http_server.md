# http_server

Receive JSON data via HTTP POST requests. This input starts an HTTP server that accepts JSON objects or arrays and feeds them into the processing pipeline.

=== "Basic"
    ```yml
    input:
      http_server:
        port: 8080
    ```

=== "Custom Path"
    ```yml
    input:
      http_server:
        address: "0.0.0.0"
        port: 9000
        path: "/events"
    ```

=== "Full Configuration"
    ```yml
    input:
      http_server:
        address: "127.0.0.1"
        port: 8080
        path: "/ingest"
        max_body_size: 5242880
        acknowledgment: true
        cors_enabled: true
    ```

## Fields

### `address`

IP address to bind the server to.

Type: `string`
Required: `false`
Default: `"0.0.0.0"`

### `port`

Port number for the HTTP server.

Type: `integer`
Required: `false`
Default: `8080`

### `path`

URL path for the ingestion endpoint.

Type: `string`
Required: `false`
Default: `"/ingest"`

Must start with `/`.

### `max_body_size`

Maximum request body size in bytes.

Type: `integer`
Required: `false`
Default: `10485760` (10 MB)

Requests exceeding this limit receive a 413 Payload Too Large response.

### `acknowledgment`

Wait for processing completion before responding.

Type: `boolean`
Required: `false`
Default: `true`

When `true`, the server waits for the message to be fully processed before sending the HTTP response. When `false`, it responds immediately after accepting the message.

### `cors_enabled`

Enable Cross-Origin Resource Sharing (CORS) headers.

Type: `boolean`
Required: `false`
Default: `false`

When enabled, allows requests from any origin with any method and headers.

## Endpoints

### POST `{path}`

The main ingestion endpoint accepts JSON data.

**Request:**
- Content-Type: `application/json`
- Body: JSON object or array of objects

**Response:**
- `200 OK`: Message(s) accepted/processed
- `400 Bad Request`: Invalid JSON
- `413 Payload Too Large`: Body exceeds max size
- `500 Internal Server Error`: Processing failed

**Success Response:**
```json
{
  "accepted": 2,
  "processed": 2
}
```

**Partial Failure Response:**
```json
{
  "accepted": 3,
  "processed": 2,
  "errors": ["Processing failed for message 1: ..."]
}
```

### GET `/health`

Health check endpoint.

**Response:**
```json
{"status": "healthy"}
```

## How It Works

1. The server binds to the configured address and port
2. POST requests to the configured path are parsed as JSON
3. Single objects become one message; arrays become multiple messages
4. Messages are fed into the pipeline (optionally waiting for acknowledgment)
5. HTTP response indicates success/failure

## Examples

### Basic Event Ingestion

```yml
input:
  http_server:
    port: 8080
    path: "/events"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "received_at", now());
        this = bytes(str(data));

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "events"
```

Send data:
```bash
curl -X POST http://localhost:8080/events \
  -H "Content-Type: application/json" \
  -d '{"event": "click", "user_id": 123}'
```

### Batch Ingestion

```yml
input:
  http_server:
    port: 8080
    path: "/batch"
    max_body_size: 52428800  # 50 MB

output:
  kafka:
    brokers: ["kafka:9092"]
    topic: "events"
```

Send array:
```bash
curl -X POST http://localhost:8080/batch \
  -H "Content-Type: application/json" \
  -d '[{"id": 1}, {"id": 2}, {"id": 3}]'
```

### Fire-and-Forget Mode

```yml
input:
  http_server:
    port: 8080
    acknowledgment: false  # Respond immediately

output:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    key: "events"
```

Useful when low latency is more important than confirmation.

### With CORS for Browser Clients

```yml
input:
  http_server:
    port: 8080
    path: "/api/events"
    cors_enabled: true

processors:
  - filter:
      condition: "event_type != null"

output:
  stdout: {}
```

### Localhost Only

```yml
input:
  http_server:
    address: "127.0.0.1"  # Only accept local connections
    port: 8080

output:
  stdout: {}
```

### Webhook Receiver

```yml
input:
  http_server:
    port: 9000
    path: "/webhook"
    acknowledgment: true

processors:
  - transform:
      mappings:
        - source: "data.object"
          target: "payload"
        - source: "type"
          target: "event_type"

output:
  amqp:
    url: "amqp://localhost"
    exchange: "webhooks"
```

## Request Format

### Single Object

```json
{"event": "click", "user_id": 123}
```

Produces one message.

### Array of Objects

```json
[
  {"event": "click", "user_id": 123},
  {"event": "view", "user_id": 456}
]
```

Produces two messages (processed independently).

### Invalid Formats

- Non-JSON body: 400 Bad Request
- Array with non-object elements: Elements are skipped
- Primitive values: 400 Bad Request

## Acknowledgment Modes

### acknowledgment: true (default)

```
Client ──POST──► Server ──► Pipeline ──► Output
  ▲                                         │
  └─────────── 200 OK ◄────────────────────┘
```

- Client waits for full processing
- Errors are reported in the response
- Higher latency but guaranteed delivery

### acknowledgment: false

```
Client ──POST──► Server ──► Pipeline ──► Output
  ▲               │
  └── 200 OK ◄────┘
```

- Client gets immediate response
- Processing continues in background
- Lower latency but no error feedback

## Error Handling

| Error | HTTP Status | Cause |
|-------|-------------|-------|
| Invalid JSON | 400 | Malformed JSON in request body |
| Payload Too Large | 413 | Body exceeds `max_body_size` |
| Processing Failed | 500 | Pipeline returned error (with `acknowledgment: true`) |

## Performance Considerations

- **Max body size**: Set appropriately for your use case
- **Acknowledgment**: Disable for lower latency if errors can be tolerated
- **Connection handling**: Uses Axum with efficient async I/O

## See Also

- [http output](../outputs/http.md) - Send data to HTTP endpoints
