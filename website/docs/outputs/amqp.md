# amqp

Publish messages to an AMQP 0-9-1 exchange. This output connects to RabbitMQ or other AMQP-compatible message brokers and publishes messages with configurable routing.

=== "Basic"
    ```yml
    output:
      amqp:
        url: "amqp://guest:guest@localhost:5672/%2f"
        exchange: "events"
    ```

=== "With Routing"
    ```yml
    output:
      amqp:
        url: "amqp://user:password@rabbitmq.example.com:5672/vhost"
        exchange: "orders"
        routing_key: "orders.processed"
        persistent: true
    ```

=== "With Batching"
    ```yml
    output:
      amqp:
        url: "amqp://localhost"
        exchange: "logs"
        routing_key: "fiddler.output"
        batch:
          size: 500
          duration: "10s"
    ```

## Fields

### `url`

The AMQP connection URL including credentials and virtual host.

Type: `string`
Required: `true`

Format: `amqp://[username:password@]host[:port]/[vhost]`

### `exchange`

The name of the exchange to publish messages to.

Type: `string`
Required: `true`

The exchange must already exist on the broker; this output does not declare exchanges.

### `routing_key`

The routing key used when publishing messages.

Type: `string`
Required: `false`
Default: `""`

The routing key determines how messages are routed to queues bound to the exchange, depending on the exchange type:
- **direct**: Exact match routing
- **topic**: Pattern-based routing with `.` delimiters
- **fanout**: Routing key is ignored
- **headers**: Routing based on message headers

### `mandatory`

When enabled, the broker will return unroutable messages.

Type: `boolean`
Required: `false`
Default: `false`

If `true` and no queue is bound to receive the message, the broker signals an error.

### `persistent`

When enabled, messages are written to disk for durability.

Type: `boolean`
Required: `false`
Default: `true`

Persistent messages survive broker restarts when published to durable queues.

### `batch`

Batching policy for grouping messages before publishing.

Type: `object`
Required: `false`

| Field | Type | Description |
|-------|------|-------------|
| `size` | integer | Maximum messages per batch (default: 500) |
| `duration` | string | Maximum time to wait before flushing (default: "10s") |

## How It Works

1. The output connects to the AMQP broker using the provided URL
2. Messages are buffered according to the batch policy
3. When a batch is ready, messages are published to the exchange
4. Each message uses the configured routing key and delivery mode
5. Automatic reconnection on connection failures

## Examples

### Basic Publishing

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "timestamp", now());
        this = bytes(str(data));

output:
  amqp:
    url: "amqp://guest:guest@localhost:5672/%2f"
    exchange: "events"
    routing_key: "events.processed"
```

### Topic Exchange Routing

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["raw_events"]

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        // Route based on event type
        let event_type = get(data, "type");

output:
  amqp:
    url: "amqp://localhost"
    exchange: "events.topic"
    routing_key: "events.${event_type}"
```

### High-Throughput Configuration

```yml
input:
  kinesis:
    stream_name: "events"

output:
  amqp:
    url: "amqp://app:secret@rabbitmq-cluster:5672/production"
    exchange: "high_volume"
    routing_key: "ingest"
    persistent: false  # Trade durability for speed
    batch:
      size: 1000
      duration: "5s"
```

### Non-Persistent for Ephemeral Data

```yml
output:
  amqp:
    url: "amqp://localhost"
    exchange: "metrics"
    routing_key: "system.metrics"
    persistent: false  # Metrics can be lost on restart
    batch:
      size: 200
      duration: "1s"
```

## Delivery Modes

| Mode | `persistent` | Use Case |
|------|--------------|----------|
| Transient | `false` | High-throughput, non-critical data |
| Persistent | `true` (default) | Important data that must survive restarts |

Persistent messages are only fully durable when:
1. The exchange is durable
2. The queue is durable
3. The message delivery mode is persistent

## Error Handling

- **Connection failures**: Automatic reconnection with backoff
- **Publish failures**: Error logged, processing continues
- **Unroutable messages**: Silently dropped unless `mandatory: true`

## Connection URL Format

| Component | Example | Description |
|-----------|---------|-------------|
| Protocol | `amqp://` | Standard AMQP (or `amqps://` for TLS) |
| Credentials | `user:pass@` | Username and password |
| Host | `localhost` | Broker hostname or IP |
| Port | `:5672` | AMQP port (5672 default) |
| Virtual Host | `/%2f` | URL-encoded virtual host |

## See Also

- [amqp input](../inputs/amqp.md) - Consume messages from AMQP queues
