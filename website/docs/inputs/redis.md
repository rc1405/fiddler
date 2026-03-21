# redis

Consume data from Redis using list operations (BLPOP/BRPOP), Pub/Sub subscriptions, or streams with consumer groups (XREADGROUP). This input supports queue-like consumption from lists, real-time message streaming via Pub/Sub, and at-least-once delivery with acknowledgment via Redis Streams.

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

=== "Stream Mode"
    ```yml
    input:
      redis:
        url: "redis://localhost:6379"
        mode: "stream"
        streams:
          - "my_stream"
        consumer_group: "my_group"
        consumer_name: "worker-1"
    ```

=== "Stream with Auto-Claim"
    ```yml
    input:
      redis:
        url: "redis://localhost:6379"
        mode: "stream"
        streams:
          - "my_stream"
        consumer_group: "my_group"
        stream_read_count: 100
        block_ms: 5000
        auto_claim:
          enabled: true
          idle_ms: 30000
          interval_ms: 10000
          batch_size: 100
        create_group: true
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

=== "With Retry"
    ```yml
    input:
      retry:
        max_retries: 3
        initial_wait: "1s"
        backoff: "exponential"
      redis:
        url: "redis://localhost:6379"
        mode: "list"
        keys:
          - "input_queue"
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
| `stream` | Read from streams using consumer groups (XREADGROUP) with at-least-once delivery |

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

### `streams`

Stream names to read from (stream mode only).

Type: `array[string]`
Required: Required for `stream` mode

### `consumer_group`

Consumer group name for stream mode.

Type: `string`
Required: Required for `stream` mode

### `consumer_name`

Consumer name within the group.

Type: `string`
Required: `false`
Default: `hostname:pid`

Defaults to `hostname:pid` if not specified. Supports Handlebars templating (e.g. `"{{HOSTNAME}}:{{PID}}"`).

### `stream_read_count`

Maximum entries per XREADGROUP call.

Type: `integer`
Required: `false`
Default: `100`

### `block_ms`

How long XREADGROUP blocks waiting for new messages, in milliseconds.

Type: `integer`
Required: `false`
Default: `5000`

### `auto_claim`

Controls periodic reclamation of idle pending messages from dead consumers using XAUTOCLAIM (requires Redis 6.2+).

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | boolean | `true` | Enable auto-claim background task |
| `idle_ms` | integer | `30000` | Minimum idle time (ms) before a message is eligible for reclamation |
| `interval_ms` | integer | `10000` | How often the auto-claim task runs (ms) |
| `batch_size` | integer | `100` | Maximum messages to reclaim per cycle |

Set `idle_ms` high enough to avoid stealing in-flight messages from healthy consumers.

### `create_group`

Automatically create the consumer group (and stream via MKSTREAM) on startup.

Type: `boolean`
Required: `false`
Default: `true`

If `false` and the group or stream doesn't exist, startup fails with a validation error.

### `use_patterns`

Enable pattern matching for Pub/Sub subscriptions.

Type: `boolean`
Required: `false`
Default: `false`

When enabled, channel names are treated as patterns (uses PSUBSCRIBE).

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

## How It Works

### List Mode

1. Connects to Redis using the provided URL
2. Executes BRPOP/BLPOP on the specified keys with timeout
3. Returns the first available message from any key
4. Repeats indefinitely until shutdown

### Stream Mode

1. Connects to Redis and creates the consumer group if `create_group: true`
2. Reads any pending messages for this consumer first (crash recovery)
3. Enters the main read loop using XREADGROUP with BLOCK
4. When a message is processed by the output, XACK is sent to acknowledge it
5. Unacknowledged messages remain in the pending entry list (PEL) and are reprocessed on restart
6. If auto-claim is enabled, a background task periodically runs XAUTOCLAIM to reclaim idle messages from dead consumers

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

### Stream Consumer Group

```yml
input:
  redis:
    url: "redis://localhost:6379"
    mode: "stream"
    streams:
      - "order_events"
    consumer_group: "order_processors"
    block_ms: 5000
    auto_claim:
      enabled: true
      idle_ms: 30000
    create_group: true

processors:
  - fiddlerscript:
      code: |
        let order = parse_json(this);
        // Process order...
        this = bytes(str(order));

output:
  http:
    url: "https://api.example.com/orders"
```

Messages are acknowledged after successful output processing. If a consumer crashes, pending messages are automatically reclaimed by other consumers via XAUTOCLAIM.

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

## List vs Pub/Sub vs Stream

| Feature | List Mode | Pub/Sub Mode | Stream Mode |
|---------|-----------|--------------|-------------|
| Persistence | Until consumed | Ephemeral | Persistent with trimming |
| Multiple consumers | Load distributed | All receive all | Consumer groups distribute load |
| Ordering | FIFO/LIFO guaranteed | Not guaranteed | Guaranteed within stream |
| Backpressure | Natural (list grows) | Messages dropped if slow | Natural (stream grows) |
| Replay | Not possible (consumed) | Not possible | Possible (stream persists) |
| Acknowledgment | Implicit (pop removes) | None | Explicit (XACK) |
| Dead consumer recovery | N/A | N/A | Auto-claim from PEL |

### When to Use List Mode
- Simple job queues and task processing
- Load distribution across workers
- No need for message replay

### When to Use Pub/Sub
- Real-time notifications
- Event broadcasting
- Fan-out patterns

### When to Use Stream Mode
- At-least-once delivery guarantees
- Consumer group coordination across instances
- Dead consumer recovery without message loss
- Message replay and audit trails

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

- [redis output](../outputs/redis.md) - Push data to Redis lists, publish to channels, or write to streams
