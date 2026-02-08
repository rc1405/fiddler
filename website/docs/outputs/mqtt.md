# mqtt

Publish messages to an MQTT topic. This output connects to an MQTT broker and publishes messages to a specified topic using the MQTT 3.1.1 protocol.

=== "Basic"
    ```yml
    output:
      mqtt:
        broker: "tcp://localhost:1883"
        topic: "events/processed"
    ```

=== "With QoS and Retain"
    ```yml
    output:
      mqtt:
        broker: "tcp://broker.example.com:1883"
        topic: "sensors/latest"
        qos: 2
        retain: true
    ```

=== "With Authentication"
    ```yml
    output:
      mqtt:
        broker: "tcp://mqtt.example.com:1883"
        client_id: "fiddler-publisher"
        topic: "data/output"
        username: "user"
        password: "secret"
    ```

## Fields

### `broker`

The MQTT broker URL.

Type: `string`
Required: `true`

Format: `tcp://host[:port]` or `mqtt://host[:port]`

Default port is 1883 if not specified.

### `topic`

The MQTT topic to publish messages to.

Type: `string`
Required: `true`

Unlike input topics, output topics cannot contain wildcards.

### `client_id`

Client identifier sent to the broker.

Type: `string`
Required: `false`
Default: Auto-generated UUID (`fiddler_<uuid>`)

Must be unique across all clients connected to the broker.

### `qos`

Quality of Service level for published messages.

Type: `integer`
Required: `false`
Default: `1`

| Value | Description |
|-------|-------------|
| `0` | At most once - fire and forget |
| `1` | At least once - acknowledged delivery |
| `2` | Exactly once - assured delivery |

### `retain`

When enabled, the broker stores the message and delivers it to new subscribers.

Type: `boolean`
Required: `false`
Default: `false`

Retained messages are useful for:
- Last known state values
- Configuration data
- Status indicators

### `username`

Username for broker authentication.

Type: `string`
Required: `false`

### `password`

Password for broker authentication.

Type: `string`
Required: `false`

## How It Works

1. The output connects to the MQTT broker
2. An event loop is spawned to handle acknowledgments and reconnections
3. Each message is published to the configured topic
4. The QoS level determines the delivery guarantee

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
        data = set(data, "processed", true);
        this = bytes(str(data));

output:
  mqtt:
    broker: "tcp://localhost:1883"
    topic: "events/processed"
```

### Sensor Data with Retain

```yml
input:
  mqtt:
    broker: "tcp://localhost:1883"
    topics:
      - "sensors/raw/+"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "timestamp", now());
        this = bytes(str(data));

output:
  mqtt:
    broker: "tcp://localhost:1883"
    topic: "sensors/processed"
    retain: true  # Last value available to new subscribers
    qos: 1
```

### Home Automation Command Relay

```yml
input:
  http_server:
    port: 9000
    path: "/command"

output:
  mqtt:
    broker: "tcp://homeassistant.local:1883"
    topic: "homeassistant/switch/relay/set"
    qos: 2  # Exactly once for commands
```

### Secure Publishing

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["internal-events"]

output:
  mqtt:
    broker: "tcp://mqtt.example.com:1883"
    client_id: "fiddler-bridge"
    topic: "external/events"
    username: "bridge"
    password: "${MQTT_PASSWORD}"
    qos: 1
```

### Status Updates

```yml
input:
  stdin: {}

processors:
  - fiddlerscript:
      code: |
        let status = #{"status": "online", "timestamp": now()};
        this = bytes(str(status));

output:
  mqtt:
    broker: "tcp://localhost:1883"
    topic: "services/fiddler/status"
    retain: true  # Persist status
    qos: 1
```

## Quality of Service Levels

| QoS | Delivery | Latency | Use Case |
|-----|----------|---------|----------|
| 0 | At most once | Lowest | High-frequency telemetry |
| 1 | At least once | Medium | Most applications |
| 2 | Exactly once | Highest | Critical commands, transactions |

## Retained Messages

When `retain: true`:
- The broker stores the last message for the topic
- New subscribers immediately receive the retained message
- Sending an empty message clears the retained message

Use cases:
- Device status (online/offline)
- Current sensor values
- Configuration state

## Error Handling

- **Connection failures**: Automatic reconnection with backoff
- **Publish failures**: Error returned to pipeline
- **Broker disconnection**: Event loop handles reconnection

## Broker URL Format

```
tcp://host:port    # Standard MQTT (port 1883)
mqtt://host:port   # Alias for tcp://
host:port          # Protocol prefix optional
host               # Uses default port 1883
```

## See Also

- [mqtt input](../inputs/mqtt.md) - Subscribe to MQTT topics
