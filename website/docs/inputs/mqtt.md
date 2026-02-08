# mqtt

Subscribe to MQTT topics and receive messages. This input connects to an MQTT broker and subscribes to one or more topics using the MQTT 3.1.1 protocol.

=== "Basic"
    ```yml
    input:
      mqtt:
        broker: "tcp://localhost:1883"
        topics:
          - "sensors/+"
    ```

=== "With Authentication"
    ```yml
    input:
      mqtt:
        broker: "tcp://broker.example.com:1883"
        topics:
          - "events/#"
          - "logs/+"
        username: "user"
        password: "secret"
        qos: 2
    ```

=== "Multiple Topics"
    ```yml
    input:
      mqtt:
        broker: "tcp://localhost:1883"
        client_id: "fiddler-subscriber"
        topics:
          - "home/+/temperature"
          - "home/+/humidity"
          - "alerts/#"
        qos: 1
    ```

## Fields

### `broker`

The MQTT broker URL.

Type: `string`
Required: `true`

Format: `tcp://host[:port]` or `mqtt://host[:port]`

Default port is 1883 if not specified.

### `topics`

List of MQTT topic patterns to subscribe to.

Type: `array[string]`
Required: `true`

Supports MQTT wildcards:
- `+` matches a single topic level
- `#` matches any number of levels (must be last character)

### `client_id`

Client identifier sent to the broker.

Type: `string`
Required: `false`
Default: Auto-generated UUID (`fiddler_<uuid>`)

Must be unique across all clients connected to the broker.

### `qos`

Quality of Service level for subscriptions.

Type: `integer`
Required: `false`
Default: `1`

| Value | Description |
|-------|-------------|
| `0` | At most once - fire and forget |
| `1` | At least once - acknowledged delivery |
| `2` | Exactly once - assured delivery |

### `username`

Username for broker authentication.

Type: `string`
Required: `false`

### `password`

Password for broker authentication.

Type: `string`
Required: `false`

### `keep_alive_secs`

Keep-alive interval in seconds.

Type: `integer`
Required: `false`
Default: `60`

The client sends ping requests to maintain the connection when idle.

## How It Works

1. The input connects to the MQTT broker
2. It subscribes to all configured topics with the specified QoS
3. Messages are received asynchronously and fed into the pipeline
4. Automatic reconnection occurs on connection failures

## MQTT Wildcards

### Single-Level Wildcard (+)

Matches exactly one topic level:

```yml
topics:
  - "sensors/+/temperature"  # Matches sensors/room1/temperature, sensors/room2/temperature
  - "home/+/+/status"        # Matches home/floor1/room1/status
```

### Multi-Level Wildcard (#)

Matches any number of levels (must be at the end):

```yml
topics:
  - "sensors/#"              # Matches sensors/temp, sensors/room/temp, etc.
  - "home/floor1/#"          # Matches all topics under home/floor1/
```

## Examples

### IoT Sensor Data

```yml
input:
  mqtt:
    broker: "tcp://localhost:1883"
    topics:
      - "sensors/+/temperature"
      - "sensors/+/humidity"
    qos: 1

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "received_at", now());
        this = bytes(str(data));

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "sensor_readings"
```

### Secure Connection with Auth

```yml
input:
  mqtt:
    broker: "tcp://mqtt.example.com:1883"
    client_id: "fiddler-production"
    topics:
      - "events/#"
    qos: 2
    username: "fiddler"
    password: "${MQTT_PASSWORD}"
    keep_alive_secs: 30

output:
  kafka:
    brokers: ["kafka:9092"]
    topic: "mqtt-events"
```

### Home Automation

```yml
input:
  mqtt:
    broker: "tcp://homeassistant.local:1883"
    topics:
      - "homeassistant/sensor/+/state"
      - "homeassistant/binary_sensor/+/state"
      - "homeassistant/switch/+/state"

processors:
  - filter:
      condition: "state != 'unavailable'"

output:
  stdout: {}
```

### All Messages (Development)

```yml
input:
  mqtt:
    broker: "tcp://localhost:1883"
    topics:
      - "#"  # Subscribe to everything
    qos: 0  # Fire and forget for debugging

output:
  stdout: {}
```

## Quality of Service Levels

| QoS | Delivery | Duplicates | Use Case |
|-----|----------|------------|----------|
| 0 | At most once | No | Telemetry, non-critical data |
| 1 | At least once | Possible | Most applications |
| 2 | Exactly once | No | Critical data, transactions |

Higher QoS levels add latency due to additional handshakes.

## Error Handling

- **Connection failures**: Automatic reconnection with 1-second backoff
- **Subscribe failures**: Logged per topic
- **Broker disconnection**: Automatic reconnection

## Broker URL Format

```
tcp://host:port    # Standard MQTT (port 1883)
mqtt://host:port   # Alias for tcp://
host:port          # Protocol prefix optional
host               # Uses default port 1883
```

## See Also

- [mqtt output](../outputs/mqtt.md) - Publish messages to MQTT topics
