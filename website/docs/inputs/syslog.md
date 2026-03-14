# syslog

Receive log data via syslog protocol over UDP, TCP, or TLS. Supports both RFC 5424 (IETF) and RFC 3164 (BSD) message formats. Compatible with rsyslog, syslog-ng, and other standard syslog forwarders.

=== "UDP (default)"
    ```yml
    input:
      syslog:
        port: 1514
    ```

=== "TCP"
    ```yml
    input:
      syslog:
        port: 1514
        transport: "tcp"
    ```

=== "TLS"
    ```yml
    input:
      syslog:
        port: 6514
        transport: "tls"
        tls:
          cert: "/etc/fiddler/server.crt"
          key: "/etc/fiddler/server.key"
    ```

=== "TLS (inline PEM)"
    ```yml
    input:
      syslog:
        port: 6514
        transport: "tls"
        tls:
          cert: |
            -----BEGIN CERTIFICATE-----
            MIIBxTCCAW...
            -----END CERTIFICATE-----
          key: |
            -----BEGIN PRIVATE KEY-----
            MIIEvQ...
            -----END PRIVATE KEY-----
    ```

=== "Full Configuration"
    ```yml
    input:
      syslog:
        address: "0.0.0.0"
        port: 1514
        transport: "tcp"
        framing: "auto"
        max_connections: 512
        connection_timeout: 60
        allow_cidrs:
          - "10.0.0.0/8"
          - "192.168.0.0/16"
        max_message_size: 65536
        socket_receive_buffer: 4194304
        channel_buffer_size: 10000
    ```

## Fields

### `address`

IP address to bind the listener to.

Type: `string`
Required: `false`
Default: `"0.0.0.0"`

### `port`

Port number for the syslog listener.

Type: `integer`
Required: `false`
Default: `1514`

The default is `1514` (unprivileged). To use the standard syslog port `514`, the process requires `CAP_NET_BIND_SERVICE` or root privileges on Linux.

### `transport`

Network transport protocol.

Type: `string`
Required: `false`
Default: `"udp"`

| Value | Description |
|-------|-------------|
| `udp` | Stateless, best-effort delivery. Most common for syslog. |
| `tcp` | Reliable, connection-oriented. Supports framed messages. |
| `tls` | TCP with TLS encryption (RFC 5425). Requires certificate configuration. |

### `framing`

TCP/TLS message framing method per RFC 6587.

Type: `string`
Required: `false`
Default: `"auto"`

| Value | Description |
|-------|-------------|
| `auto` | Auto-detect per connection from the first byte |
| `octet_counting` | Length-prefixed: `<length> <message>` — robust, no ambiguity |
| `newline` | Newline-delimited — legacy default used by rsyslog |

Only applies to TCP and TLS transports. When set to `auto`, the first byte of each connection determines the framing: a digit selects octet-counting, `<` selects newline-delimited.

### `max_connections`

Maximum number of concurrent TCP/TLS connections.

Type: `integer`
Required: `false`
Default: `512`

Connections beyond this limit are rejected with a warning log. Minimum value: `1`.

### `connection_timeout`

Idle connection timeout in seconds.

Type: `integer`
Required: `false`
Default: `60`

TCP/TLS connections that send no data within this period are closed. Protects against slowloris-style attacks. Minimum value: `1`.

### `allow_cidrs`

IP/CIDR allowlist for source addresses.

Type: `array[string]`
Required: `false`
Default: `[]` (allow all)

When non-empty, only connections from IPs matching at least one CIDR are accepted. Others are rejected with a warning log.

```yml
allow_cidrs:
  - "10.0.0.0/8"
  - "192.168.1.0/24"
  - "172.16.0.0/12"
```

### `tls`

TLS configuration block. Required when `transport` is `"tls"`.

Type: `object`
Required: Required when `transport` is `"tls"`

Each string field (`cert`, `key`, `ca`) accepts either a **file path** or **inline PEM content**. If the value starts with `-----BEGIN`, it is treated as inline PEM; otherwise it is treated as a file path.

#### `tls.cert`

Server certificate — file path or inline PEM.

Type: `string`
Required: `true` (when `tls` is present)

#### `tls.key`

Server private key — file path or inline PEM.

Type: `string`
Required: `true` (when `tls` is present)

The module warns at startup if the key file permissions are more open than `0600`.

#### `tls.ca`

CA certificate for verifying client certificates — file path or inline PEM.

Type: `string`
Required: Required when `tls.client_auth` is not `"none"`

#### `tls.client_auth`

Client certificate authentication mode.

Type: `string`
Required: `false`
Default: `"none"`

| Value | Description |
|-------|-------------|
| `none` | No client certificate required |
| `optional` | Client certificate is requested but not required |
| `required` | Client must present a valid certificate signed by the CA |

When set to `"optional"` or `"required"`, `tls.ca` must also be provided.

### `max_message_size`

Maximum syslog message size in bytes.

Type: `integer`
Required: `false`
Default: `65536` (64 KB)

Messages exceeding this limit are rejected. Valid range: 256 to 16,777,216 (16 MB).

For octet-counting framing, the declared length is checked before allocating memory. For newline framing, reads are bounded to this size.

### `socket_receive_buffer`

UDP socket receive buffer size hint in bytes.

Type: `integer`
Required: `false`
Default: `4194304` (4 MB)

Passed to the OS as `SO_RCVBUF`. The actual buffer size may be limited by OS settings (`net.core.rmem_max` on Linux). Only applies to UDP transport.

### `channel_buffer_size`

Internal channel capacity between the listener and the pipeline.

Type: `integer`
Required: `false`
Default: `10000`

When the channel is full, incoming messages are dropped with a warning log.

## Message Output

The parsed syslog **message body** is placed in `Message.bytes`. Syslog envelope fields are stored in `Message.metadata`.

### Metadata Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `syslog_facility` | string | Facility name | `"kern"`, `"local0"` |
| `syslog_facility_code` | integer | Facility numeric code | `0`, `16` |
| `syslog_severity` | string | Severity name | `"err"`, `"info"` |
| `syslog_severity_code` | integer | Severity numeric code | `3`, `6` |
| `syslog_timestamp` | string | Timestamp (RFC 3339 when available) | `"2024-01-15T10:30:00+00:00"` |
| `syslog_hostname` | string | Source hostname or IP | `"myhost"` |
| `syslog_appname` | string | Application name | `"sshd"` |
| `syslog_procid` | string | Process ID | `"1234"` |
| `syslog_msgid` | string | Message type identifier (RFC 5424) | `"ID47"` |
| `syslog_version` | integer | Syslog protocol version (RFC 5424) | `1` |
| `syslog_structured_data` | map | Parsed structured data elements (RFC 5424) | `{"exampleSDID@32473": {"iut": "3"}}` |
| `syslog_format` | string | Detected message format | `"rfc5424"` or `"rfc3164"` |
| `syslog_source_ip` | string | Sender IP address | `"10.0.0.1"` |
| `syslog_raw` | string | Original raw syslog line | Full syslog frame |

Fields with NIL values (`-` in RFC 5424) are omitted from metadata.

## How It Works

1. The listener binds to the configured address and port
2. For UDP: datagrams are received and parsed individually
3. For TCP/TLS: connections are accepted and framed messages are read
4. Each syslog message is parsed using both RFC 5424 and RFC 3164 formats (best match)
5. The message body goes into `Message.bytes`; envelope fields go into `Message.metadata`
6. Messages are sent to the processing pipeline

## Examples

### Receive from rsyslog (UDP)

```yml
input:
  syslog:
    port: 1514

processors:
  - fiddlerscript:
      code: |
        let msg = str(this);
        let data = {};
        data = set(data, "message", msg);
        this = bytes(str(data));

output:
  stdout: {}
```

rsyslog configuration:
```
*.* @fiddler-host:1514
```

### Receive from rsyslog (TCP)

```yml
input:
  syslog:
    port: 1514
    transport: "tcp"
    framing: "auto"

output:
  elasticsearch:
    urls:
      - "http://localhost:9200"
    index: "syslog-%Y.%m.%d"
```

rsyslog configuration:
```
*.* @@fiddler-host:1514
```

### TLS with Mutual Authentication

```yml
input:
  syslog:
    port: 6514
    transport: "tls"
    tls:
      cert: "/etc/fiddler/server.crt"
      key: "/etc/fiddler/server.key"
      ca: "/etc/fiddler/ca.crt"
      client_auth: "required"

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "secure_logs"
```

### Restricted Source IPs

```yml
input:
  syslog:
    port: 1514
    transport: "tcp"
    allow_cidrs:
      - "10.0.0.0/8"
      - "192.168.1.0/24"
    max_connections: 256

output:
  stdout: {}
```

Only accepts connections from `10.0.0.0/8` and `192.168.1.0/24`.

### High-Volume UDP with Increased Buffer

```yml
input:
  syslog:
    port: 1514
    transport: "udp"
    socket_receive_buffer: 16777216  # 16 MB
    max_message_size: 8192
    channel_buffer_size: 50000

output:
  file:
    path: "/var/log/fiddler/syslog.log"
```

For high-volume environments, also increase the OS limit:
```bash
sysctl -w net.core.rmem_max=16777216
```

### Filter by Severity

```yml
input:
  syslog:
    port: 1514

processors:
  - filter:
      condition: "syslog_severity_code <= `4`"

output:
  stdout: {}
```

Only forwards messages with severity `warning` or higher (lower numeric code = higher severity).

### Route by Facility

```yml
input:
  syslog:
    port: 1514
    transport: "tcp"

processors:
  - switch:
      - condition: "syslog_facility == 'auth' || syslog_facility == 'authpriv'"
        processors:
          - fiddlerscript:
              code: |
                let data = {};
                data = set(data, "type", "security");
                data = set(data, "message", str(this));
                this = bytes(str(data));

output:
  stdout: {}
```

## Transport Comparison

| Feature | UDP | TCP | TLS |
|---------|-----|-----|-----|
| Reliability | Best-effort | Reliable | Reliable |
| Ordering | Not guaranteed | Per-connection | Per-connection |
| Encryption | None | None | TLS 1.2+ |
| Source IP trust | Spoofable | Trustworthy | Trustworthy |
| RFC | RFC 5426 | RFC 6587 | RFC 5425 |
| Default port | 514 / 1514 | 514 / 1514 | 6514 |
| Connection overhead | None | Per-connection | Per-connection + handshake |

### When to Use UDP
- High-volume logging where occasional message loss is acceptable
- Legacy environments where TCP syslog is not supported
- Low-latency requirements

### When to Use TCP
- Reliable delivery is required
- Messages may exceed the typical UDP MTU (~1500 bytes)
- Firewall environments that block UDP

### When to Use TLS
- Logs traverse untrusted networks
- Compliance requirements mandate encryption
- Client authentication is needed (mTLS)

## TCP Framing

TCP syslog uses framing to separate messages on a stream. Two methods are defined in RFC 6587:

### Octet-Counting

```
<length> <message>
```

The message is preceded by its byte length as ASCII digits followed by a space. This is unambiguous and handles messages containing newlines.

Example: `70 <165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 - Hello world`

### Newline-Delimited

```
<message>\n
```

Messages are separated by newline characters. This is the legacy default used by rsyslog. Messages cannot contain embedded newlines.

### Auto-Detection

When `framing: "auto"` (default), the first byte of each connection determines the method:

- Digit (`0`-`9`): octet-counting
- `<`: newline-delimited (syslog messages start with `<priority>`)
- Other: connection rejected

## Security Considerations

- **UDP source IPs are spoofable** — do not rely on `syslog_source_ip` from UDP for authentication or authorization
- **All metadata fields are untrusted** — they come from the network and may contain malicious content
- **Use `allow_cidrs`** to restrict which hosts can send logs
- **Use TLS** when logs traverse untrusted networks
- **Use `tls.client_auth: "required"`** for mutual TLS to authenticate senders
- **`max_connections` and `connection_timeout`** protect against connection exhaustion attacks
- **`max_message_size`** prevents memory exhaustion from oversized messages
- **Privileged ports** — port 514 requires `CAP_NET_BIND_SERVICE` or root:
  ```bash
  sudo setcap 'cap_net_bind_service=+ep' /usr/local/bin/fiddler-cli
  ```

## Syslog Format Reference

### RFC 5424 (IETF)

```
<priority>version timestamp hostname appname procid msgid [structured-data] message
```

Example:
```
<165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 [exampleSDID@32473 iut="3"] Application started
```

### RFC 3164 (BSD)

```
<priority>timestamp hostname tag: message
```

Example:
```
<34>Oct 11 22:14:15 mymachine sshd[1234]: Failed password for invalid user
```

The parser automatically detects the format and sets the `syslog_format` metadata field accordingly.

## Forwarder Configuration

### rsyslog

```
# UDP forwarding
*.* @fiddler-host:1514

# TCP forwarding
*.* @@fiddler-host:1514

# TCP with octet-counting
$ActionSendStreamDriver gtls
$ActionSendStreamDriverMode 1
*.* @@fiddler-host:6514;RSYSLOG_SyslogProtocol23Format
```

### syslog-ng

```
destination d_fiddler {
    syslog("fiddler-host"
        port(1514)
        transport("tcp")
    );
};

log {
    source(s_sys);
    destination(d_fiddler);
};
```

### systemd-journald

```
# /etc/systemd/journal-remote.conf
[Remote]
URL=fiddler-host:1514
```

Or use `logger` for testing:
```bash
logger -n fiddler-host -P 1514 --tcp "Test message from logger"
```

## Error Handling

| Condition | Behavior |
|-----------|----------|
| Channel full | Message dropped, warning logged |
| Oversized message | Message rejected, warning logged |
| Connection limit reached | Connection rejected, warning logged |
| Idle timeout | Connection closed, warning logged |
| Invalid framing | Connection closed, warning logged |
| TLS handshake failure | Connection closed, warning logged |
| Non-allowed source IP | Connection/datagram rejected, warning logged |

## Performance Considerations

- **`socket_receive_buffer`**: Increase for high-volume UDP to reduce kernel drops. Also increase the OS limit (`net.core.rmem_max`).
- **`channel_buffer_size`**: Increase to absorb bursts without dropping messages.
- **`max_connections`**: Set based on the expected number of concurrent senders.
- **UDP vs TCP**: UDP has lower overhead but no delivery guarantees. TCP adds per-connection state.
- **`num_threads`** (pipeline-level): Controls parallelism for processing received messages.
