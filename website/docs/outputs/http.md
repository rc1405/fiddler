# http

Send data to HTTP endpoints via POST or PUT requests. This output supports authentication, custom headers, and batching with configurable payload formats.

=== "Basic"
    ```yml
    output:
      http:
        url: "https://api.example.com/events"
    ```

=== "With Headers"
    ```yml
    output:
      http:
        url: "https://api.example.com/data"
        method: "POST"
        headers:
          Content-Type: "application/json"
          X-API-Version: "v2"
    ```

=== "With Batching"
    ```yml
    output:
      http:
        url: "https://api.example.com/bulk"
        batch:
          size: 100
          duration: "5s"
          format: "json_array"
    ```

## Fields

### `url`

Target HTTP endpoint URL.

Type: `string`
Required: `true`

Must be a valid HTTP or HTTPS URL.

### `method`

HTTP method to use.

Type: `string`
Required: `false`
Default: `"POST"`

Allowed values: `"POST"`, `"PUT"`

### `headers`

Custom HTTP headers to include in requests.

Type: `object`
Required: `false`

Key-value pairs of header names and values.

### `auth`

Authentication configuration.

Type: `object`
Required: `false`

#### Basic Authentication

```yml
auth:
  type: "basic"
  username: "user"
  password: "secret"
```

#### Bearer Token

```yml
auth:
  type: "bearer"
  token: "your-api-token"
```

### `timeout_secs`

Request timeout in seconds.

Type: `integer`
Required: `false`
Default: `30`

### `batch`

Batching configuration. When present, enables batch mode.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `size` | integer | 500 | Maximum messages per batch |
| `duration` | string | "10s" | Maximum time before flush |
| `format` | string | "ndjson" | Batch payload format |

#### Batch Formats

| Format | Description |
|--------|-------------|
| `ndjson` | Newline-delimited JSON (one JSON per line) |
| `json_array` | JSON array of objects |

## How It Works

### Single Message Mode (no batch)

1. Each message is sent as an individual HTTP request
2. The message bytes become the request body
3. Response status is checked (non-2xx is an error)

### Batch Mode (with batch config)

1. Messages are buffered until batch size or duration is reached
2. Messages are formatted according to the batch format
3. A single HTTP request is sent with the batch payload
4. Response status is checked

## Examples

### Simple POST

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["events"]

output:
  http:
    url: "https://api.example.com/events"
    headers:
      Content-Type: "application/json"
```

### With Bearer Token Auth

```yml
input:
  http_server:
    port: 8080

output:
  http:
    url: "https://api.example.com/ingest"
    auth:
      type: "bearer"
      token: "${API_TOKEN}"
    headers:
      Content-Type: "application/json"
```

### With Basic Auth

```yml
output:
  http:
    url: "https://api.example.com/data"
    auth:
      type: "basic"
      username: "service"
      password: "${API_PASSWORD}"
```

### Batched NDJSON

```yml
input:
  kinesis:
    stream_name: "events"

output:
  http:
    url: "https://api.example.com/bulk"
    batch:
      size: 1000
      duration: "10s"
      format: "ndjson"
    headers:
      Content-Type: "application/x-ndjson"
```

Request body:
```
{"event":"click","user":1}
{"event":"view","user":2}
{"event":"click","user":3}
```

### Batched JSON Array

```yml
output:
  http:
    url: "https://api.example.com/batch"
    batch:
      size: 100
      duration: "5s"
      format: "json_array"
    headers:
      Content-Type: "application/json"
```

Request body:
```json
[
  {"event":"click","user":1},
  {"event":"view","user":2},
  {"event":"click","user":3}
]
```

### Webhook Delivery

```yml
input:
  amqp:
    url: "amqp://localhost"
    queue: "notifications"

output:
  http:
    url: "https://webhook.example.com/notify"
    method: "POST"
    headers:
      Content-Type: "application/json"
      X-Webhook-Secret: "${WEBHOOK_SECRET}"
    timeout_secs: 10
```

### PUT Method

```yml
output:
  http:
    url: "https://api.example.com/resource"
    method: "PUT"
    headers:
      Content-Type: "application/json"
```

## Batch Format Comparison

| Format | Content-Type | Use Case |
|--------|--------------|----------|
| `ndjson` | `application/x-ndjson` | Streaming ingestion, log aggregators |
| `json_array` | `application/json` | APIs expecting array payloads |

### NDJSON Format

```
{"id":1,"name":"Alice"}
{"id":2,"name":"Bob"}
```

- Each message on its own line
- No wrapping array
- Easy to stream and parse line-by-line

### JSON Array Format

```json
[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]
```

- Valid JSON array
- Messages are parsed and re-serialized
- Compatible with standard JSON APIs

## Duration Syntax

The `batch.duration` field accepts duration strings:

| Format | Example | Description |
|--------|---------|-------------|
| Milliseconds | `100ms` | 100 milliseconds |
| Seconds | `5s` | 5 seconds |
| Minutes | `2m` | 2 minutes |
| Hours | `1h` | 1 hour |

## Error Handling

- **Connection failures**: Error returned to pipeline
- **Non-2xx responses**: Error with status code and body
- **Timeout**: Error after configured timeout

## Response Handling

Successful responses (2xx) are considered successful. Error responses include:
- HTTP status code
- Response body (for debugging)

## Performance Considerations

- **Batching**: Significantly reduces HTTP overhead for high-volume data
- **Connection pooling**: Reuses connections to the same host
- **Timeout**: Set appropriately for your endpoint's latency

## See Also

- [http_server input](../inputs/http_server.md) - Receive data via HTTP
