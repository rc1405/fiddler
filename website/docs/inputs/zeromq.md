# zeromq

Receive messages using ZeroMQ sockets. This input supports PULL and SUB socket types for building distributed messaging patterns.

=== "Pull Socket"
    ```yml
    input:
      zeromq:
        socket_type: "pull"
        bind: "tcp://*:5555"
    ```

=== "Subscribe Socket"
    ```yml
    input:
      zeromq:
        socket_type: "sub"
        connect:
          - "tcp://publisher:5555"
        subscribe:
          - "events"
          - "logs"
    ```

=== "Connect to Multiple"
    ```yml
    input:
      zeromq:
        socket_type: "pull"
        connect:
          - "tcp://worker1:5555"
          - "tcp://worker2:5555"
          - "tcp://worker3:5555"
    ```

## Fields

### `socket_type`

The type of ZeroMQ socket to create.

Type: `string`
Required: `true`

| Value | Description |
|-------|-------------|
| `pull` | Receive messages from PUSH sockets (pipeline pattern) |
| `sub` | Subscribe to topics from PUB sockets (pub/sub pattern) |

### `bind`

Address to bind the socket to.

Type: `string`
Required: `false` (but either `bind` or `connect` must be specified)

Format: `<transport>://<address>`

### `connect`

Addresses to connect to.

Type: `array[string]`
Required: `false` (but either `bind` or `connect` must be specified)

Multiple addresses can be specified to connect to several endpoints.

### `subscribe`

Topic filters for SUB sockets.

Type: `array[string]`
Required: Required for `sub` socket type

Use `""` (empty string) to receive all messages.

## How It Works

### PULL Socket

1. Creates a PULL socket and binds/connects to the specified addresses
2. Messages are received from connected PUSH sockets
3. Messages are load-balanced across multiple PUSH senders
4. Fair-queuing ensures even distribution

### SUB Socket

1. Creates a SUB socket and binds/connects to the specified addresses
2. Subscribes to the specified topic filters
3. Messages matching any filter are received
4. Topic matching is prefix-based

## Socket Patterns

### Pipeline (PUSH/PULL)

Distributes work across multiple workers:

```
         ┌──► [PULL Worker 1]
[PUSH] ──┼──► [PULL Worker 2]
         └──► [PULL Worker 3]
```

```yml
# Worker configuration
input:
  zeromq:
    socket_type: "pull"
    connect:
      - "tcp://dispatcher:5555"
```

### Pub/Sub

Broadcasts messages to all subscribers:

```
              ┌──► [SUB Client 1]
[PUB] ───────┼──► [SUB Client 2]
              └──► [SUB Client 3]
```

```yml
# Subscriber configuration
input:
  zeromq:
    socket_type: "sub"
    connect:
      - "tcp://publisher:5555"
    subscribe:
      - "events"
```

## Examples

### Work Distribution Pipeline

```yml
# Collector receiving from multiple producers
input:
  zeromq:
    socket_type: "pull"
    bind: "tcp://*:5555"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "collector_ts", now());
        this = bytes(str(data));

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "events"
```

### Subscribe to All Messages

```yml
input:
  zeromq:
    socket_type: "sub"
    connect:
      - "tcp://publisher:5555"
    subscribe:
      - ""  # Empty string = all messages

output:
  stdout: {}
```

### Multi-Topic Subscription

```yml
input:
  zeromq:
    socket_type: "sub"
    connect:
      - "tcp://events.example.com:5555"
    subscribe:
      - "order:"
      - "payment:"
      - "shipment:"

processors:
  - filter:
      condition: "type != null"

output:
  kafka:
    brokers: ["kafka:9092"]
    topic: "events"
```

### Connect to Multiple Publishers

```yml
input:
  zeromq:
    socket_type: "sub"
    connect:
      - "tcp://server1:5555"
      - "tcp://server2:5555"
      - "tcp://server3:5555"
    subscribe:
      - "metrics"

output:
  stdout: {}
```

### Bind vs Connect

Both SUB and PULL sockets can either bind or connect:

```yml
# Publisher binds, subscriber connects (common)
input:
  zeromq:
    socket_type: "sub"
    connect:
      - "tcp://publisher:5555"
    subscribe:
      - ""
```

```yml
# Subscriber binds, publisher connects (reverse)
input:
  zeromq:
    socket_type: "sub"
    bind: "tcp://*:5556"
    subscribe:
      - ""
```

## Transport Types

| Transport | Format | Description |
|-----------|--------|-------------|
| TCP | `tcp://host:port` | Network communication |
| IPC | `ipc:///path` | Inter-process (Unix sockets) |
| inproc | `inproc://name` | In-process threads |

### TCP Examples

```yml
bind: "tcp://*:5555"              # Bind to all interfaces
bind: "tcp://127.0.0.1:5555"      # Localhost only
connect:
  - "tcp://remote:5555"           # Connect to remote host
```

### IPC Examples

```yml
bind: "ipc:///tmp/fiddler.sock"
connect:
  - "ipc:///tmp/producer.sock"
```

## Topic Matching (SUB)

Topic filtering uses prefix matching:

| Subscribe | Matches |
|-----------|---------|
| `""` | All messages |
| `"order"` | `"order"`, `"order:new"`, `"order:update"` |
| `"order:"` | `"order:new"`, `"order:update"` (not `"order"`) |
| `"error"` | `"error"`, `"error.critical"`, `"errors"` |

## Error Handling

- **Bind failures**: Error returned during startup
- **Connect failures**: Logged, connection retried automatically
- **Receive errors**: Logged, continues listening

## Performance Considerations

- **High-water mark**: ZeroMQ buffers messages; backpressure is handled internally
- **Multiple connections**: Fair-queuing across all connected sockets
- **Message framing**: ZeroMQ handles message boundaries automatically

## See Also

- [zeromq output](../outputs/zeromq.md) - Send messages using ZeroMQ sockets
- [ZeroMQ Guide](https://zguide.zeromq.org/) - Comprehensive ZeroMQ patterns
