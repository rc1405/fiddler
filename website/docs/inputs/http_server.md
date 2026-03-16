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

=== "With Basic Auth"
    ```yml
    input:
      http_server:
        port: 8080
        path: "/events"
        auth:
          type: basic
          username: admin
          password: secret123
    ```

=== "With Bearer Auth"
    ```yml
    input:
      http_server:
        port: 8080
        path: "/events"
        auth:
          type: bearer
          token: "my-api-token"
    ```

=== "With Path Parameters"
    ```yml
    input:
      http_server:
        port: 8080
        path: "/ingest/:logtype"
    ```

=== "Multiple Path Parameters"
    ```yml
    input:
      http_server:
        port: 8080
        path: "/ingest/:source/:logtype"
    ```

=== "With TLS"
    ```yml
    input:
      http_server:
        port: 8443
        path: "/events"
        tls:
          cert: "/etc/fiddler/server.crt"
          key: "/etc/fiddler/server.key"
    ```

=== "TLS with mTLS"
    ```yml
    input:
      http_server:
        port: 8443
        tls:
          cert: "/etc/fiddler/server.crt"
          key: "/etc/fiddler/server.key"
          ca: "/etc/fiddler/ca.crt"
          client_auth: "required"
    ```

=== "With Retry"
    ```yml
    input:
      retry:
        max_retries: 3
        initial_wait: "1s"
        backoff: "exponential"
      http_server:
        port: 8080
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

URL path for the ingestion endpoint. Supports path parameters using `:name` syntax (e.g. `/ingest/:logtype`). Path parameter values are added to each message's metadata.

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

### `auth`

Authentication for incoming requests. When configured, all requests to the ingestion endpoint must include valid credentials. The health endpoint (`/health`) remains unauthenticated.

Type: `object`
Required: `false`

#### `auth.type`

Authentication type.

Type: `string`
Required: `true` (when `auth` is present)

| Value | Description |
|-------|-------------|
| `basic` | HTTP Basic authentication |
| `bearer` | Bearer token authentication |

#### `auth.username`

Username for basic authentication.

Type: `string`
Required: Required when `auth.type` is `"basic"`

#### `auth.password`

Password for basic authentication.

Type: `string`
Required: Required when `auth.type` is `"basic"`

#### `auth.token`

Token for bearer authentication.

Type: `string`
Required: Required when `auth.type` is `"bearer"`

Unauthenticated requests receive a `401 Unauthorized` response:
```json
{"error": "Unauthorized"}
```

### `tls`

TLS configuration for HTTPS. When present, the server accepts HTTPS connections instead of plain HTTP.

!!! note "Feature Flag Required"
    TLS support requires both the `http_server` and `tls` features to be enabled when building fiddler.
    ```bash
    cargo build --features http_server,tls
    ```

Type: `object`
Required: `false`

Each string field (`cert`, `key`, `ca`) accepts either a **file path** or **inline PEM content**. If the value starts with `-----BEGIN`, it is treated as inline PEM.

#### `tls.cert`

Server certificate — file path or inline PEM.

Type: `string`
Required: `true` (when `tls` is present)

#### `tls.key`

Server private key — file path or inline PEM.

Type: `string`
Required: `true` (when `tls` is present)

#### `tls.ca`

CA certificate for verifying client certificates — file path or inline PEM.

Type: `string`
Required: Required when `tls.client_auth` is not `"none"`

#### `tls.client_auth`

Client certificate authentication mode.

Type: `string`
Required: `false`
Default: `"none"`

| Value | Description |
|-------|-------------|
| `none` | No client certificate required |
| `optional` | Client certificate is requested but not required |
| `required` | Client must present a valid certificate signed by the CA |

When set to `"optional"` or `"required"`, `tls.ca` must also be provided.

### `retry`

Retry policy for failed reads. When present, the runtime retries failed reads with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |

## Endpoints

### POST `{path}`

The main ingestion endpoint accepts JSON data.

**Request:**
- Content-Type: `application/json`
- Body: JSON object or array of objects

**Response:**
- `200 OK`: Message(s) accepted/processed
- `401 Unauthorized`: Missing or invalid credentials (when `auth` is configured)
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

### HTTPS Server

```yml
input:
  http_server:
    port: 8443
    path: "/events"
    tls:
      cert: "/etc/fiddler/server.crt"
      key: "/etc/fiddler/server.key"

output:
  stdout: {}
```

Send data:
```bash
curl -X POST https://localhost:8443/events \
  -H "Content-Type: application/json" \
  --cacert /etc/fiddler/ca.crt \
  -d '{"event": "click"}'
```

### Path Parameter Routing

Use path parameters to classify incoming data by URL segment. Each parameter is added to the message metadata.

```yml
input:
  http_server:
    port: 8080
    path: "/ingest/:source/:logtype"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let meta = metadata();
        data = set(data, "source", meta.source);
        data = set(data, "logtype", meta.logtype);
        this = bytes(str(data));

output:
  stdout: {}
```

Send data:
```bash
# metadata: { "source": "firewall", "logtype": "network" }
curl -X POST http://localhost:8080/ingest/firewall/network \
  -H "Content-Type: application/json" \
  -d '{"event": "connection", "ip": "10.0.0.1"}'
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
| Unauthorized | 401 | Missing or invalid credentials (when `auth` is configured) |
| Invalid JSON | 400 | Malformed JSON in request body |
| Payload Too Large | 413 | Body exceeds `max_body_size` |
| Processing Failed | 500 | Pipeline returned error (with `acknowledgment: true`) |

## Performance Considerations

- **Max body size**: Set appropriately for your use case
- **Acknowledgment**: Disable for lower latency if errors can be tolerated
- **Connection handling**: Uses Axum with efficient async I/O
- **TLS**: Adds per-connection handshake overhead; use when security is required

## See Also

- [http output](../outputs/http.md) - Send data to HTTP endpoints
