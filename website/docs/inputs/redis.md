# redis

Consume data from Redis using list operations (BLPOP/BRPOP) or Pub/Sub subscriptions. This input supports both queue-like consumption from lists and real-time message streaming via Pub/Sub.

=== "List Mode"
    ```yml
    input:
      redis:
        url: "redis://localhost:6379"
        mode: "list"
        keys:
          - "input_queue"
    ```

=== "Pub/Sub Mode"
    ```yml
    input:
      redis:
        url: "redis://localhost:6379"
        mode: "pubsub"
        channels:
          - "events"
          - "notifications"
    ```

=== "Pattern Subscribe"
    ```yml
    input:
      redis:
        url: "redis://localhost:6379"
        mode: "pubsub"
        channels:
          - "events.*"
          - "logs.*"
        use_patterns: true
    ```

## Fields

### `url`

Redis connection URL.

Type: `string`
Required: `true`

Format: `redis://[username:password@]host[:port][/db]`

### `mode`

Operation mode for consuming data.

Type: `string`
Required: `false`
Default: `"list"`

| Value | Description |
|-------|-------------|
| `list` | Pop messages from Redis lists using BLPOP/BRPOP |
| `pubsub` | Subscribe to channels for real-time messages |

### `keys`

List key names to pop from (list mode only).

Type: `array[string]`
Required: Required for `list` mode

Multiple keys are supported; the first key with data is popped.

### `channels`

Channel names or patterns to subscribe to (pubsub mode only).

Type: `array[string]`
Required: Required for `pubsub` mode

### `list_command`

The blocking list operation to use.

Type: `string`
Required: `false`
Default: `"brpop"`

| Value | Description |
|-------|-------------|
| `brpop` | Pop from the tail (FIFO queue behavior) |
| `blpop` | Pop from the head (LIFO stack behavior) |

### `timeout`

Blocking timeout in seconds for list operations.

Type: `integer`
Required: `false`
Default: `1`

### `use_patterns`

Enable pattern matching for Pub/Sub subscriptions.

Type: `boolean`
Required: `false`
Default: `false`

When enabled, channel names are treated as patterns (uses PSUBSCRIBE).

## How It Works

### List Mode

1. Connects to Redis using the provided URL
2. Executes BRPOP/BLPOP on the specified keys with timeout
3. Returns the first available message from any key
4. Repeats indefinitely until shutdown

### Pub/Sub Mode

1. Connects to Redis and creates a Pub/Sub connection
2. Subscribes to all specified channels (or patterns if `use_patterns: true`)
3. Messages are received asynchronously as they're published
4. Automatic reconnection on connection failures

## Examples

### Simple Queue Processing

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    keys:
      - "job_queue"

processors:
  - fiddlerscript:
      code: |
        let job = parse_json(this);
        // Process job...
        this = bytes(str(job));

output:
  stdout: {}
```

### Priority Queue with Multiple Keys

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    keys:
      - "high_priority"
      - "normal_priority"
      - "low_priority"
    list_command: "brpop"
    timeout: 5

output:
  http:
    url: "https://api.example.com/process"
```

Keys are checked in order; first non-empty key wins.

### Real-Time Event Streaming

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "pubsub"
    channels:
      - "events"
      - "notifications"

output:
  kafka:
    brokers: ["kafka:9092"]
    topic: "redis-events"
```

### Pattern-Based Subscriptions

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "pubsub"
    channels:
      - "sensor:*:temperature"
      - "sensor:*:humidity"
    use_patterns: true

processors:
  - transform:
      mappings:
        - source: "value"
          target: "reading"
        - source: "timestamp"
          target: "ts"

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "sensor_data"
```

### Stack Processing (LIFO)

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "list"
    keys:
      - "task_stack"
    list_command: "blpop"  # Pop from head = LIFO

output:
  stdout: {}
```

### With Authentication

```yml
input:
  redis:
    url: "redis://default:mypassword@redis.example.com:6379/0"
    mode: "list"
    keys:
      - "secure_queue"

output:
  stdout: {}
```

## List vs Pub/Sub

| Feature | List Mode | Pub/Sub Mode |
|---------|-----------|--------------|
| Persistence | Messages persist until consumed | Messages are ephemeral |
| Multiple consumers | Load distributed | All consumers receive all messages |
| Ordering | FIFO/LIFO guaranteed | Order not guaranteed |
| Backpressure | Natural (list grows) | Messages dropped if slow |
| Replay | Possible (data persists) | Not possible |

### When to Use List Mode
- Job queues and task processing
- Reliable message delivery
- Load distribution across workers

### When to Use Pub/Sub
- Real-time notifications
- Event broadcasting
- Fan-out patterns

## Pattern Syntax (Pub/Sub)

When `use_patterns: true`:

| Pattern | Matches |
|---------|---------|
| `sensor:*` | `sensor:1`, `sensor:room1`, etc. |
| `logs:*:error` | `logs:app:error`, `logs:db:error` |
| `event:?` | `event:1`, `event:a` (single char) |
| `data:[abc]` | `data:a`, `data:b`, `data:c` |

## Error Handling

- **Connection failures**: Automatic reconnection with backoff
- **List timeout**: Returns to blocking wait
- **Pub/Sub disconnection**: Stream ends, reconnection attempted

## Redis URL Format

```
redis://localhost                    # Localhost, default port
redis://host:6380                    # Custom port
redis://user:pass@host:6379          # With authentication
redis://host:6379/1                  # Database 1
redis://:password@host:6379/0        # Password only (no username)
```

## See Also

- [redis output](../outputs/redis.md) - Push data to Redis lists or publish to channels
