# zeromq

Send messages using ZeroMQ sockets. This output supports PUSH and PUB socket types for building distributed messaging patterns.

=== "Push Socket"
    ```yml
    output:
      zeromq:
        socket_type: "push"
        connect:
          - "tcp://collector:5555"
    ```

=== "Publish Socket"
    ```yml
    output:
      zeromq:
        socket_type: "pub"
        bind: "tcp://*:5555"
    ```

=== "Connect to Multiple"
    ```yml
    output:
      zeromq:
        socket_type: "push"
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
| `push` | Send messages to PULL sockets (pipeline pattern) |
| `pub` | Publish messages to SUB sockets (pub/sub pattern) |

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

## How It Works

### PUSH Socket

1. Creates a PUSH socket and binds/connects to the specified addresses
2. Messages are sent to connected PULL sockets
3. Load-balancing distributes messages across multiple receivers
4. Back-pressure is handled by ZeroMQ's high-water mark

### PUB Socket

1. Creates a PUB socket and binds/connects to the specified addresses
2. All messages are broadcast to all connected SUB sockets
3. Topic filtering happens at the subscriber side
4. No persistence - messages to unconnected subscribers are dropped

## Socket Patterns

### Pipeline (PUSH/PULL)

Distributes work across multiple workers:

```
[Fiddler PUSH] ──┬──► [PULL Worker 1]
                 ├──► [PULL Worker 2]
                 └──► [PULL Worker 3]
```

```yml
output:
  zeromq:
    socket_type: "push"
    bind: "tcp://*:5555"
```

### Pub/Sub

Broadcasts messages to all subscribers:

```
[Fiddler PUB] ───┬──► [SUB Client 1]
                 ├──► [SUB Client 2]
                 └──► [SUB Client 3]
```

```yml
output:
  zeromq:
    socket_type: "pub"
    bind: "tcp://*:5555"
```

## Examples

### Work Distribution

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "dispatched_at", now());
        this = bytes(str(data));

output:
  zeromq:
    socket_type: "push"
    bind: "tcp://*:5555"
```

Workers connect with PULL sockets to receive work.

### Event Broadcasting

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["events"]

output:
  zeromq:
    socket_type: "pub"
    bind: "tcp://*:5556"
```

Any number of SUB clients can connect to receive all events.

### Push to Collector

```yml
input:
  mqtt:
    broker: "tcp://localhost:1883"
    topics:
      - "sensors/#"

output:
  zeromq:
    socket_type: "push"
    connect:
      - "tcp://collector:5555"
```

### Load Balancing Across Workers

```yml
input:
  amqp:
    url: "amqp://localhost"
    queue: "tasks"

output:
  zeromq:
    socket_type: "push"
    connect:
      - "tcp://worker1:5555"
      - "tcp://worker2:5555"
      - "tcp://worker3:5555"
```

Messages are distributed round-robin across workers.

### Bind vs Connect

Both PUSH and PUB sockets can either bind or connect:

```yml
# Producer binds, workers connect (common)
output:
  zeromq:
    socket_type: "push"
    bind: "tcp://*:5555"
```

```yml
# Workers bind, producer connects (reverse)
output:
  zeromq:
    socket_type: "push"
    connect:
      - "tcp://worker1:5555"
      - "tcp://worker2:5555"
```

### Topic Prefixing for Pub/Sub

Add topic prefixes to messages for subscriber filtering:

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let topic = get(data, "type") ?? "default";
        // Prefix message with topic for subscriber filtering
        this = bytes(topic + ":" + str(data));

output:
  zeromq:
    socket_type: "pub"
    bind: "tcp://*:5555"
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
bind: "tcp://0.0.0.0:5555"        # Same as above
connect:
  - "tcp://remote:5555"           # Connect to remote host
```

### IPC Examples

```yml
bind: "ipc:///tmp/fiddler-out.sock"
connect:
  - "ipc:///tmp/collector.sock"
```

## PUSH vs PUB

| Feature | PUSH | PUB |
|---------|------|-----|
| Delivery | One receiver per message | All subscribers |
| Load balancing | Yes (round-robin) | No |
| Slow receiver | Back-pressure | Messages dropped |
| Use case | Work distribution | Broadcasting |

## Error Handling

- **Bind failures**: Error returned during startup
- **Connect failures**: Logged, connection retried automatically
- **Send failures**: Error returned to pipeline

## Performance Considerations

- **High-water mark**: ZeroMQ buffers messages; configure carefully for backpressure
- **Multiple connections**: PUSH load-balances, PUB broadcasts to all
- **No persistence**: Messages are ephemeral

## See Also

- [zeromq input](../inputs/zeromq.md) - Receive messages using ZeroMQ sockets
- [ZeroMQ Guide](https://zguide.zeromq.org/) - Comprehensive ZeroMQ patterns
