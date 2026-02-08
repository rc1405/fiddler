# amqp

Consume messages from an AMQP 0-9-1 queue. This input connects to RabbitMQ or other AMQP-compatible message brokers and reads messages from a specified queue.

=== "Basic"
    ```yml
    input:
      amqp:
        url: "amqp://guest:guest@localhost:5672/%2f"
        queue: "events"
    ```

=== "With Prefetch"
    ```yml
    input:
      amqp:
        url: "amqp://user:password@rabbitmq.example.com:5672/vhost"
        queue: "orders"
        prefetch_count: 50
        consumer_tag: "fiddler-consumer"
    ```

=== "Auto-Acknowledge"
    ```yml
    input:
      amqp:
        url: "amqp://localhost"
        queue: "logs"
        auto_ack: true
    ```

## Fields

### `url`

The AMQP connection URL including credentials and virtual host.

Type: `string`
Required: `true`

Format: `amqp://[username:password@]host[:port]/[vhost]`

The virtual host must be URL-encoded (e.g., `/` becomes `%2f`).

### `queue`

The name of the queue to consume messages from.

Type: `string`
Required: `true`

The queue must already exist on the broker; this input does not declare queues.

### `consumer_tag`

An identifier for this consumer. Used by the broker to track the consumer.

Type: `string`
Required: `false`
Default: `"fiddler"`

### `prefetch_count`

The maximum number of unacknowledged messages to buffer from the broker.

Type: `integer`
Required: `false`
Default: `100`

Higher values improve throughput but increase memory usage. Lower values provide better load distribution across multiple consumers.

### `auto_ack`

When enabled, messages are acknowledged immediately upon receipt. When disabled (default), messages are acknowledged after successful processing.

Type: `boolean`
Required: `false`
Default: `false`

**Warning**: Enabling `auto_ack` means messages may be lost if processing fails.

## How It Works

1. The input connects to the AMQP broker using the provided URL
2. It sets the QoS prefetch count for flow control
3. Messages are consumed from the specified queue
4. With `auto_ack: false` (default), messages are acknowledged after successful processing
5. If the connection drops, automatic reconnection is attempted

## Message Acknowledgment

By default, the AMQP input uses manual acknowledgment:

- Messages are acknowledged after they successfully pass through the pipeline
- If processing fails, messages are returned to the queue (nack with requeue)
- On graceful shutdown, pending messages are requeued

With `auto_ack: true`:
- Messages are removed from the queue immediately upon delivery
- No redelivery occurs on processing failure
- Use only when message loss is acceptable

## Error Handling

- **Connection failures**: Automatic reconnection with 5-second backoff
- **Channel errors**: Automatic recovery with 1-second backoff
- **Delivery errors**: Connection is reset and consumption resumes

## Examples

### Basic Event Processing

```yml
input:
  amqp:
    url: "amqp://guest:guest@localhost:5672/%2f"
    queue: "events"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "processed_at", now());
        this = bytes(str(data));

output:
  stdout: {}
```

### High-Throughput Configuration

```yml
input:
  amqp:
    url: "amqp://app:secret@rabbitmq-cluster:5672/production"
    queue: "high_volume_events"
    prefetch_count: 500
    consumer_tag: "fiddler-worker-1"

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "events"
    batch:
      size: 1000
      duration: "5s"
```

### Multiple Consumer Setup

When running multiple Fiddler instances consuming from the same queue, use unique consumer tags:

```yml
input:
  amqp:
    url: "amqp://localhost"
    queue: "shared_queue"
    prefetch_count: 100
    consumer_tag: "fiddler-${HOSTNAME}"
```

## Connection URL Format

| Component | Example | Description |
|-----------|---------|-------------|
| Protocol | `amqp://` | Standard AMQP (or `amqps://` for TLS) |
| Credentials | `user:pass@` | Username and password |
| Host | `localhost` | Broker hostname or IP |
| Port | `:5672` | AMQP port (5672 default, 5671 for TLS) |
| Virtual Host | `/%2f` | URL-encoded virtual host |

### URL Examples

```
amqp://localhost                           # Localhost with guest credentials
amqp://user:pass@broker.example.com        # Remote broker
amqp://user:pass@broker:5672/production    # Custom port and vhost
amqps://user:pass@broker:5671/%2f          # TLS connection
```

## See Also

- [amqp output](../outputs/amqp.md) - Publish messages to AMQP exchanges
