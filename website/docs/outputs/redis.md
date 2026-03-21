# redis

Send data to Redis using list operations (LPUSH/RPUSH), Pub/Sub publishing, or stream appends (XADD). This output supports queue-like insertion into lists, real-time message broadcasting via Pub/Sub, and persistent stream writes with optional trimming.

=== "List Mode"
    ```yml
    output:
      redis:
        url: "redis://localhost:6379"
        mode: "list"
        key: "output_queue"
    ```

=== "Pub/Sub Mode"
    ```yml
    output:
      redis:
        url: "redis://localhost:6379"
        mode: "pubsub"
        channel: "events"
    ```

=== "Stream Mode"
    ```yml
    output:
      redis:
        url: "redis://localhost:6379"
        mode: "stream"
        stream: "my_stream"
    ```

=== "Stream with Trimming"
    ```yml
    output:
      redis:
        url: "redis://localhost:6379"
        mode: "stream"
        stream: "my_stream"
        max_len: 10000
        batch:
          size: 500
          duration: "10s"
    ```

=== "With Batching"
    ```yml
    output:
      redis:
        url: "redis://localhost:6379"
        mode: "list"
        key: "batch_queue"
        batch:
          size: 500
          duration: "5s"
    ```

=== "With Retry"
    ```yml
    output:
      retry:
        max_retries: 5
        initial_wait: "2s"
        backoff: "exponential"
      redis:
        url: "redis://localhost:6379"
        mode: "list"
        key: "output_queue"
    ```

## Fields

### `url`

Redis connection URL.

Type: `string`
Required: `true`

Format: `redis://[username:password@]host[:port][/db]`

### `mode`

Operation mode for sending data.

Type: `string`
Required: `false`
Default: `"list"`

| Value | Description |
|-------|-------------|
| `list` | Push messages to a Redis list using LPUSH/RPUSH |
| `pubsub` | Publish messages to a channel |
| `stream` | Append messages to a stream using XADD |

### `key`

List key name to push to (list mode only).

Type: `string`
Required: Required for `list` mode

### `channel`

Channel name to publish to (pubsub mode only).

Type: `string`
Required: Required for `pubsub` mode

### `stream`

Stream name to write to (stream mode only).

Type: `string`
Required: Required for `stream` mode

### `max_len`

Approximate maximum stream length. When set, every XADD includes `MAXLEN ~ <max_len>` to cap stream size. Redis trims approximately — the actual length may exceed this value slightly.

Type: `integer`
Required: `false`

### `list_command`

The list operation to use.

Type: `string`
Required: `false`
Default: `"rpush"`

| Value | Description |
|-------|-------------|
| `rpush` | Push to the tail (queue behavior) |
| `lpush` | Push to the head (stack behavior) |

### `batch`

Batching policy for grouping messages (list and stream modes).

Type: `object`
Required: `false`

| Field | Type | Description |
|-------|------|-------------|
| `size` | integer | Maximum messages per batch (default: 500) |
| `duration` | string | Maximum time before flush (default: "10s") |
| `max_batch_bytes` | integer | Maximum cumulative byte size per batch (default: 10MB) |

**Note**: Batching is supported in list and stream modes. It is not supported in pubsub mode and will be ignored.

### `retry`

Retry policy for failed writes. When present, the runtime retries failed writes with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |

Authentication failures are never retried.

## How It Works

### List Mode

1. Messages are buffered according to the batch policy
2. When a batch is ready, messages are pushed using Redis pipelining
3. RPUSH adds to the tail (FIFO queue behavior)
4. LPUSH adds to the head (LIFO stack behavior)

### Stream Mode

1. Messages are appended to the stream using XADD with auto-generated entry IDs
2. Message bytes are stored in the `data` field; metadata entries are stored with `meta:` prefix
3. If `max_len` is set, approximate trimming (`MAXLEN ~`) keeps the stream capped
4. When batching is enabled, multiple XADD commands are pipelined in a single network round-trip

### Pub/Sub Mode

1. Each message is published individually to the channel
2. Messages are delivered to all subscribed clients
3. No persistence - undelivered messages are lost

## Examples

### Simple Queue Output

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "processed_at", now());
        this = bytes(str(data));

output:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    key: "processed_events"
```

### High-Throughput with Batching

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["events"]

output:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    key: "event_queue"
    list_command: "rpush"
    batch:
      size: 1000
      duration: "5s"
```

### Stream with Trimming

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["events"]

output:
  redis:
    url: "redis://localhost:6379"
    mode: "stream"
    stream: "event_stream"
    max_len: 100000
    batch:
      size: 500
      duration: "5s"
```

Stream entries persist and can be consumed by multiple consumer groups. The `max_len` keeps the stream from growing unbounded.

### Event Broadcasting

```yml
input:
  http_server:
    port: 8080
    path: "/broadcast"

output:
  redis:
    url: "redis://localhost:6379"
    mode: "pubsub"
    channel: "notifications"
```

### Stack (LIFO) Output

```yml
input:
  stdin: {}

output:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    key: "task_stack"
    list_command: "lpush"  # New items at head = LIFO
```

### Authenticated Connection

```yml
input:
  kinesis:
    stream_name: "events"

output:
  redis:
    url: "redis://default:secretpassword@redis.example.com:6379/0"
    mode: "list"
    key: "kinesis_events"
    batch:
      size: 500
      duration: "10s"
```

### Multi-Channel Publishing

For publishing to multiple channels, use multiple outputs:

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        this = bytes(str(data));

output:
  switch:
    cases:
      - check: "type == 'alert'"
        output:
          redis:
            url: "redis://localhost:6379"
            mode: "pubsub"
            channel: "alerts"
      - output:
          redis:
            url: "redis://localhost:6379"
            mode: "pubsub"
            channel: "events"
```

## List vs Pub/Sub vs Stream

| Feature | List Mode | Pub/Sub Mode | Stream Mode |
|---------|-----------|--------------|-------------|
| Persistence | Yes | No | Yes (with optional trimming) |
| Batching | Supported | Not supported | Supported |
| Delivery | Guaranteed | Best effort | Guaranteed |
| Consumers | One consumes each message | All receive all | Consumer groups distribute load |
| Backpressure | List grows | Slow consumers miss messages | Stream grows (trimmable) |
| Replay | Not possible | Not possible | Possible |

### When to Use List Mode
- Simple job queues and task distribution
- Buffering between services
- No need for message replay

### When to Use Pub/Sub
- Real-time notifications
- Event broadcasting
- Fan-out patterns

### When to Use Stream Mode
- Durable event logs with consumer groups
- Multiple independent consumer groups on the same data
- Message replay and audit trails
- At-least-once delivery when paired with stream input

## Batching with Pipelining

In list and stream modes, batching uses Redis pipelining for efficiency:
- Multiple RPUSH/LPUSH or XADD commands are sent in a single network round-trip
- Reduces latency and increases throughput
- Atomic per-batch (all or nothing)

## Error Handling

- **Connection failures**: Automatic reconnection with connection manager
- **Write failures**: Error returned to pipeline
- **Reconnection**: Seamless recovery without data loss (for queued messages)

## Redis URL Format

```
redis://localhost                    # Localhost, default port
redis://host:6380                    # Custom port
redis://user:pass@host:6379          # With authentication
redis://host:6379/1                  # Database 1
redis://:password@host:6379/0        # Password only (no username)
```

## Performance Considerations

- **Batch size**: Larger batches improve throughput but increase latency
- **Pipelining**: Automatically used in list mode for batches
- **Connection pooling**: Managed internally

## See Also

- [redis input](../inputs/redis.md) - Pop data from Redis lists, subscribe to channels, or read from streams
