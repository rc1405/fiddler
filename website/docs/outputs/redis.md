# redis

Send data to Redis using list operations (LPUSH/RPUSH) or Pub/Sub publishing. This output supports both queue-like insertion into lists and real-time message broadcasting via Pub/Sub.

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

### `key`

List key name to push to (list mode only).

Type: `string`
Required: Required for `list` mode

### `channel`

Channel name to publish to (pubsub mode only).

Type: `string`
Required: Required for `pubsub` mode

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

Batching policy for grouping messages (list mode only).

Type: `object`
Required: `false`

| Field | Type | Description |
|-------|------|-------------|
| `size` | integer | Maximum messages per batch (default: 500) |
| `duration` | string | Maximum time before flush (default: "10s") |

**Note**: Batching is not supported in pubsub mode and will be ignored.

## How It Works

### List Mode

1. Messages are buffered according to the batch policy
2. When a batch is ready, messages are pushed using Redis pipelining
3. RPUSH adds to the tail (FIFO queue behavior)
4. LPUSH adds to the head (LIFO stack behavior)

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

## List vs Pub/Sub

| Feature | List Mode | Pub/Sub Mode |
|---------|-----------|--------------|
| Persistence | Yes | No |
| Batching | Supported | Not supported |
| Delivery | Guaranteed | Best effort |
| Consumers | One consumes each message | All receive all messages |
| Backpressure | List grows | Slow consumers miss messages |

### When to Use List Mode
- Job queues and task distribution
- Buffering between services
- Reliable message delivery

### When to Use Pub/Sub
- Real-time notifications
- Event broadcasting
- Fan-out patterns

## Batching with Pipelining

In list mode, batching uses Redis pipelining for efficiency:
- Multiple RPUSH/LPUSH commands are sent in a single network round-trip
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

- [redis input](../inputs/redis.md) - Pop data from Redis lists or subscribe to channels
