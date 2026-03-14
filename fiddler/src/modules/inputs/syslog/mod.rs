//! Syslog input module for receiving log data via UDP, TCP, or TLS.
//!
//! Supports both RFC 5424 (IETF) and RFC 3164 (BSD) syslog message formats.
//! Compatible with rsyslog, syslog-ng, and other standard syslog forwarders.
//!
//! # Configuration
//!
//! ```yaml
//! input:
//!   syslog:
//!     address: "0.0.0.0"             # Bind address (default: "0.0.0.0")
//!     port: 1514                     # Listen port (default: 1514)
//!     transport: "udp"               # "udp", "tcp", or "tls" (default: "udp")
//!     framing: "auto"                # "auto", "octet_counting", "newline" (default: "auto")
//!     max_connections: 512           # Max concurrent TCP/TLS connections (default: 512)
//!     connection_timeout: 60         # Idle connection timeout in seconds (default: 60)
//!     allow_cidrs: []                # IP/CIDR allowlist, empty = allow all
//!     tls:                           # TLS configuration (required for tls transport)
//!       cert: ""                     # Server certificate — file path or inline PEM
//!       key: ""                      # Server private key — file path or inline PEM
//!       ca: ""                       # CA cert for client verification (optional)
//!       client_auth: "none"          # "none", "optional", "required" (default: "none")
//!     max_message_size: 65536        # Max syslog message size in bytes (default: 64KB)
//!     socket_receive_buffer: 4194304 # UDP socket receive buffer hint (default: 4MB)
//! ```
//!
//! # Port Requirements
//!
//! The default port is `1514` (unprivileged). To use the standard syslog port
//! `514`, the process requires `CAP_NET_BIND_SERVICE` or root privileges on Linux.
//!
//! # Transports
//!
//! - **UDP** (default): Stateless, best-effort delivery. Most common for syslog.
//!   Source IP is spoofable — do not rely on `syslog_source_ip` for authentication.
//! - **TCP**: Reliable, connection-oriented. Supports octet-counting and newline framing.
//! - **TLS**: TCP with TLS encryption (RFC 5425). Requires certificate configuration.
//!   Supports mutual TLS (mTLS) via `tls_client_auth`.
//!
//! # TCP Framing
//!
//! Two framing methods are supported per RFC 6587:
//! - **Octet-counting**: `<length> <message>` — robust, no ambiguity
//! - **Newline-delimited**: message terminated by `\n` — legacy default for rsyslog
//!
//! When `framing: "auto"`, the framing method is auto-detected per connection
//! by inspecting the first byte (digit → octet-counting, `<` → newline).
//!
//! # Message Output
//!
//! The parsed syslog message body goes into `Message.bytes`. Syslog envelope
//! fields (facility, severity, hostname, etc.) are stored in `Message.metadata`.
//!
//! # Security Considerations
//!
//! All metadata fields are **untrusted input** from the network. Downstream
//! consumers must treat them accordingly. UDP source IPs are trivially spoofable.
//!
//! # Examples
//!
//! ## rsyslog forwarding (UDP)
//!
//! ```yaml
//! input:
//!   syslog:
//!     port: 1514
//!     transport: udp
//! ```
//!
//! rsyslog config: `*.* @fiddler-host:1514`
//!
//! ## TLS with client certificates
//!
//! ```yaml
//! input:
//!   syslog:
//!     port: 6514
//!     transport: tls
//!     tls:
//!       cert: /etc/fiddler/server.crt
//!       key: /etc/fiddler/server.key
//!       ca: /etc/fiddler/ca.crt
//!       client_auth: required
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::modules::tls::ServerTlsConfig;
use crate::Message;
use crate::{CallbackChan, Closer, Error, Input};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use ipnet::IpNet;
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use syslog_loose::ProcId;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

const DEFAULT_ADDRESS: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 1514;
const DEFAULT_TRANSPORT: &str = "udp";
const DEFAULT_FRAMING: &str = "auto";
const DEFAULT_MAX_CONNECTIONS: usize = 512;
const DEFAULT_CONNECTION_TIMEOUT: u64 = 60;
const DEFAULT_MAX_MESSAGE_SIZE: usize = 65536;
const DEFAULT_SOCKET_RECEIVE_BUFFER: usize = 4 * 1024 * 1024;
const DEFAULT_CHANNEL_BUFFER_SIZE: usize = 10000;

/// Maximum digits allowed in octet-counting length prefix.
const MAX_OCTET_COUNT_DIGITS: usize = 7;

/// Syslog input configuration.
#[derive(Deserialize, Clone)]
pub struct SyslogConfig {
    /// Bind address (default: "0.0.0.0").
    #[serde(default = "default_address")]
    pub address: String,
    /// Listen port (default: 1514).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Transport protocol: "udp", "tcp", or "tls" (default: "udp").
    #[serde(default = "default_transport")]
    pub transport: String,
    /// TCP framing method: "auto", "octet_counting", or "newline" (default: "auto").
    #[serde(default = "default_framing")]
    pub framing: String,
    /// Maximum concurrent TCP/TLS connections (default: 512).
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Idle connection timeout in seconds (default: 60).
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// IP/CIDR allowlist. Empty means allow all.
    #[serde(default)]
    pub allow_cidrs: Vec<String>,
    /// TLS configuration (required when transport is "tls").
    pub tls: Option<ServerTlsConfig>,
    /// Maximum syslog message size in bytes (default: 65536).
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
    /// UDP socket receive buffer size hint (default: 4MB).
    #[serde(default = "default_socket_receive_buffer")]
    pub socket_receive_buffer: usize,
    /// Internal channel capacity (default: 10000).
    #[serde(default = "default_channel_buffer_size")]
    pub channel_buffer_size: usize,
}

fn default_address() -> String {
    DEFAULT_ADDRESS.to_string()
}
fn default_port() -> u16 {
    DEFAULT_PORT
}
fn default_transport() -> String {
    DEFAULT_TRANSPORT.to_string()
}
fn default_framing() -> String {
    DEFAULT_FRAMING.to_string()
}
fn default_max_connections() -> usize {
    DEFAULT_MAX_CONNECTIONS
}
fn default_connection_timeout() -> u64 {
    DEFAULT_CONNECTION_TIMEOUT
}
fn default_max_message_size() -> usize {
    DEFAULT_MAX_MESSAGE_SIZE
}
fn default_socket_receive_buffer() -> usize {
    DEFAULT_SOCKET_RECEIVE_BUFFER
}
fn default_channel_buffer_size() -> usize {
    DEFAULT_CHANNEL_BUFFER_SIZE
}

/// TCP framing mode per RFC 6587.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Framing {
    /// Auto-detect from first byte of connection.
    Auto,
    /// `<length> <message>` framing.
    OctetCounting,
    /// Newline-delimited framing.
    Newline,
}

impl Framing {
    fn from_str(s: &str) -> Result<Self, Error> {
        match s {
            "auto" => Ok(Framing::Auto),
            "octet_counting" => Ok(Framing::OctetCounting),
            "newline" => Ok(Framing::Newline),
            _ => Err(Error::ConfigFailedValidation(format!(
                "invalid framing '{}': must be 'auto', 'octet_counting', or 'newline'",
                s
            ))),
        }
    }
}

/// Check if an IP address is in the allowlist.
fn is_allowed(addr: &IpAddr, allowed: &[IpNet]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    allowed.iter().any(|net| net.contains(addr))
}

/// Strip control characters from a string, keeping tabs and newlines.
fn strip_control_chars(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\t' || *c == '\n')
        .collect()
}

/// Convert a syslog facility to its numeric code.
fn facility_code(facility: &syslog_loose::SyslogFacility) -> u8 {
    use syslog_loose::SyslogFacility;
    match facility {
        SyslogFacility::LOG_KERN => 0,
        SyslogFacility::LOG_USER => 1,
        SyslogFacility::LOG_MAIL => 2,
        SyslogFacility::LOG_DAEMON => 3,
        SyslogFacility::LOG_AUTH => 4,
        SyslogFacility::LOG_SYSLOG => 5,
        SyslogFacility::LOG_LPR => 6,
        SyslogFacility::LOG_NEWS => 7,
        SyslogFacility::LOG_UUCP => 8,
        SyslogFacility::LOG_CRON => 9,
        SyslogFacility::LOG_AUTHPRIV => 10,
        SyslogFacility::LOG_FTP => 11,
        SyslogFacility::LOG_NTP => 12,
        SyslogFacility::LOG_AUDIT => 13,
        SyslogFacility::LOG_ALERT => 14,
        SyslogFacility::LOG_CLOCKD => 15,
        SyslogFacility::LOG_LOCAL0 => 16,
        SyslogFacility::LOG_LOCAL1 => 17,
        SyslogFacility::LOG_LOCAL2 => 18,
        SyslogFacility::LOG_LOCAL3 => 19,
        SyslogFacility::LOG_LOCAL4 => 20,
        SyslogFacility::LOG_LOCAL5 => 21,
        SyslogFacility::LOG_LOCAL6 => 22,
        SyslogFacility::LOG_LOCAL7 => 23,
    }
}

/// Convert a syslog severity to its numeric code.
fn severity_code(severity: &syslog_loose::SyslogSeverity) -> u8 {
    use syslog_loose::SyslogSeverity;
    match severity {
        SyslogSeverity::SEV_EMERG => 0,
        SyslogSeverity::SEV_ALERT => 1,
        SyslogSeverity::SEV_CRIT => 2,
        SyslogSeverity::SEV_ERR => 3,
        SyslogSeverity::SEV_WARNING => 4,
        SyslogSeverity::SEV_NOTICE => 5,
        SyslogSeverity::SEV_INFO => 6,
        SyslogSeverity::SEV_DEBUG => 7,
    }
}

/// Parse a raw syslog message into a fiddler Message with metadata.
fn parse_syslog_message(raw: &[u8], source_ip: &str) -> Message {
    let raw_str = String::from_utf8_lossy(raw);
    let raw_trimmed = raw_str.trim_end_matches('\n').trim_end_matches('\r');

    let parsed = syslog_loose::parse_message(raw_trimmed, syslog_loose::Variant::Either);

    let mut metadata: HashMap<String, Value> = HashMap::new();

    // Facility
    if let Some(ref facility) = parsed.facility {
        metadata.insert(
            "syslog_facility".into(),
            Value::String(strip_control_chars(facility.as_str())),
        );
        metadata.insert(
            "syslog_facility_code".into(),
            Value::Number(serde_yaml::Number::from(facility_code(facility) as u64)),
        );
    }

    // Severity
    if let Some(ref severity) = parsed.severity {
        metadata.insert(
            "syslog_severity".into(),
            Value::String(strip_control_chars(severity.as_str())),
        );
        metadata.insert(
            "syslog_severity_code".into(),
            Value::Number(serde_yaml::Number::from(severity_code(severity) as u64)),
        );
    }

    // Timestamp
    if let Some(ref ts) = parsed.timestamp {
        metadata.insert("syslog_timestamp".into(), Value::String(ts.to_rfc3339()));
    }

    // Hostname
    if let Some(hostname) = parsed.hostname {
        if hostname != "-" {
            metadata.insert(
                "syslog_hostname".into(),
                Value::String(strip_control_chars(hostname)),
            );
        }
    }

    // Appname
    if let Some(appname) = parsed.appname {
        if appname != "-" {
            metadata.insert(
                "syslog_appname".into(),
                Value::String(strip_control_chars(appname)),
            );
        }
    }

    // ProcID
    if let Some(ref procid) = parsed.procid {
        let procid_str = match procid {
            ProcId::PID(pid) => pid.to_string(),
            ProcId::Name(name) => (*name).to_string(),
        };
        if procid_str != "-" {
            metadata.insert(
                "syslog_procid".into(),
                Value::String(strip_control_chars(&procid_str)),
            );
        }
    }

    // MsgID
    if let Some(msgid) = parsed.msgid {
        if msgid != "-" {
            metadata.insert(
                "syslog_msgid".into(),
                Value::String(strip_control_chars(msgid)),
            );
        }
    }

    // Protocol / version / format
    match parsed.protocol {
        syslog_loose::Protocol::RFC5424(version) => {
            metadata.insert(
                "syslog_version".into(),
                Value::Number(serde_yaml::Number::from(version as u64)),
            );
            metadata.insert("syslog_format".into(), Value::String("rfc5424".into()));
        }
        syslog_loose::Protocol::RFC3164 => {
            metadata.insert("syslog_format".into(), Value::String("rfc3164".into()));
        }
    }

    // Structured data (RFC 5424)
    if !parsed.structured_data.is_empty() {
        let mut sd_map: HashMap<String, Value> = HashMap::new();
        for element in &parsed.structured_data {
            let mut params_map: HashMap<String, Value> = HashMap::new();
            for (key, value) in element.params() {
                params_map.insert(
                    strip_control_chars(key),
                    Value::String(strip_control_chars(&value)),
                );
            }
            sd_map.insert(
                strip_control_chars(element.id),
                Value::Mapping(
                    params_map
                        .into_iter()
                        .map(|(k, v)| (Value::String(k), v))
                        .collect(),
                ),
            );
        }
        metadata.insert(
            "syslog_structured_data".into(),
            Value::Mapping(
                sd_map
                    .into_iter()
                    .map(|(k, v)| (Value::String(k), v))
                    .collect(),
            ),
        );
    }

    // Source IP
    metadata.insert(
        "syslog_source_ip".into(),
        Value::String(source_ip.to_string()),
    );

    // Raw syslog line
    metadata.insert("syslog_raw".into(), Value::String(raw_trimmed.to_string()));

    // Extract message body — strip BOM if present
    let msg_body = parsed.msg.trim_start_matches('\u{FEFF}');

    Message {
        bytes: msg_body.as_bytes().to_vec(),
        metadata,
        ..Default::default()
    }
}

/// Read a syslog frame from a buffered reader using the specified framing method.
///
/// Returns `Ok(Some(data))` for a successfully read frame, `Ok(None)` at EOF,
/// or `Err` on protocol violations.
async fn read_syslog_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    framing: &mut Framing,
    max_size: usize,
) -> Result<Option<Vec<u8>>, Error> {
    if *framing == Framing::Auto {
        // Peek first byte to determine framing
        let buf = match reader.fill_buf().await {
            Ok([]) => return Ok(None),
            Ok(buf) => buf,
            Err(e) => {
                return Err(Error::ExecutionError(format!(
                    "syslog: failed to peek framing byte: {}",
                    e
                )));
            }
        };
        let first_byte = buf[0];
        if first_byte.is_ascii_digit() {
            *framing = Framing::OctetCounting;
        } else if first_byte == b'<' {
            *framing = Framing::Newline;
        } else {
            return Err(Error::ExecutionError(format!(
                "syslog: unexpected first byte 0x{:02x}, cannot auto-detect framing",
                first_byte
            )));
        }
        debug!(framing = ?framing, "auto-detected TCP framing");
    }

    match framing {
        Framing::OctetCounting => read_octet_counted_frame(reader, max_size).await,
        Framing::Newline => read_newline_frame(reader, max_size).await,
        Framing::Auto => unreachable!(),
    }
}

/// Read a frame using octet-counting: `<digits> <space> <message bytes>`.
async fn read_octet_counted_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max_size: usize,
) -> Result<Option<Vec<u8>>, Error> {
    // Read the length prefix (digits followed by space)
    let mut length_buf = Vec::with_capacity(MAX_OCTET_COUNT_DIGITS + 1);
    loop {
        let byte = match read_single_byte(reader).await {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                return Err(Error::ExecutionError(format!(
                    "syslog: octet-counting read error: {}",
                    e
                )));
            }
            None => return Ok(None), // EOF
        };

        if byte == b' ' {
            break;
        }
        if !byte.is_ascii_digit() {
            return Err(Error::ExecutionError(format!(
                "syslog: octet-counting expected digit or space, got 0x{:02x}",
                byte
            )));
        }
        length_buf.push(byte);
        if length_buf.len() > MAX_OCTET_COUNT_DIGITS {
            return Err(Error::ExecutionError(format!(
                "syslog: octet-counting length prefix exceeds {} digits",
                MAX_OCTET_COUNT_DIGITS
            )));
        }
    }

    if length_buf.is_empty() {
        return Err(Error::ExecutionError(
            "syslog: octet-counting empty length prefix".into(),
        ));
    }

    let length_str = String::from_utf8_lossy(&length_buf);
    let length: usize = length_str.parse().map_err(|e| {
        Error::ExecutionError(format!(
            "syslog: octet-counting invalid length '{}': {}",
            length_str, e
        ))
    })?;

    if length > max_size {
        return Err(Error::ExecutionError(format!(
            "syslog: octet-counting frame size {} exceeds max_message_size {}",
            length, max_size
        )));
    }

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await.map_err(|e| {
        Error::ExecutionError(format!("syslog: octet-counting read body error: {}", e))
    })?;

    Ok(Some(buf))
}

/// Read a newline-delimited frame, bounded by max_size.
async fn read_newline_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max_size: usize,
) -> Result<Option<Vec<u8>>, Error> {
    let mut buf = Vec::with_capacity(1024);
    // Use take() to bound the read
    let bytes_read = reader
        .take(max_size as u64)
        .read_until(b'\n', &mut buf)
        .await
        .map_err(|e| Error::ExecutionError(format!("syslog: newline framing read error: {}", e)))?;

    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    // Remove trailing newline
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    // Remove trailing CR
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }

    if buf.is_empty() {
        // Empty line, skip
        return Ok(Some(Vec::new()));
    }

    Ok(Some(buf))
}

/// Read a single byte from a reader.
async fn read_single_byte<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Option<Result<u8, std::io::Error>> {
    let mut buf = [0u8; 1];
    match reader.read_exact(&mut buf).await {
        Ok(_) => Some(Ok(buf[0])),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => None,
        Err(e) => Some(Err(e)),
    }
}

/// Syslog input that receives log data over UDP, TCP, or TLS.
pub struct SyslogInput {
    receiver: Receiver<Result<(Message, Option<CallbackChan>), Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

/// Shared state for listener tasks.
struct ListenerState {
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    allowed_cidrs: Vec<IpNet>,
    max_message_size: usize,
    framing: Framing,
    max_connections: usize,
    connection_timeout: u64,
    shutdown: Arc<AtomicBool>,
}

impl SyslogInput {
    /// Start the syslog input from configuration.
    fn new(
        config: SyslogConfig,
        framing: Framing,
        allowed_cidrs: Vec<IpNet>,
    ) -> Result<Self, Error> {
        let (sender, receiver) = bounded(config.channel_buffer_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let state = Arc::new(ListenerState {
            sender,
            allowed_cidrs,
            max_message_size: config.max_message_size,
            framing,
            max_connections: config.max_connections,
            connection_timeout: config.connection_timeout,
            shutdown: shutdown_flag.clone(),
        });

        let address = config.address.clone();
        let port = config.port;
        let transport = config.transport.clone();
        let socket_receive_buffer = config.socket_receive_buffer;
        let tls_config = config.tls.clone();

        tokio::spawn(async move {
            // Wait for shutdown signal in a separate task
            let shutdown_flag_clone = shutdown_flag.clone();
            tokio::spawn(async move {
                let _ = shutdown_rx.await;
                shutdown_flag_clone.store(true, Ordering::SeqCst);
            });

            let bind_addr = format!("{}:{}", address, port);

            match transport.as_str() {
                "udp" => {
                    if let Err(e) =
                        udp_listener_task(&bind_addr, socket_receive_buffer, &state).await
                    {
                        error!(error = %e, "UDP listener error");
                    }
                }
                "tcp" => {
                    if let Err(e) = tcp_listener_task(&bind_addr, &state).await {
                        error!(error = %e, "TCP listener error");
                    }
                }
                "tls" => {
                    if let Err(e) = tls_listener_task(&bind_addr, &state, tls_config.as_ref()).await
                    {
                        error!(error = %e, "TLS listener error");
                    }
                }
                _ => {
                    error!(transport = %transport, "unsupported transport");
                }
            }
        });

        info!(
            address = %config.address,
            port = config.port,
            transport = %config.transport,
            "syslog input initialized"
        );

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

#[async_trait]
impl Input for SyslogInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(result) => result,
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for SyslogInput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("syslog input closing");
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// UDP listener task — receives datagrams and parses syslog messages.
async fn udp_listener_task(
    bind_addr: &str,
    socket_receive_buffer: usize,
    state: &Arc<ListenerState>,
) -> Result<(), Error> {
    // Create socket via socket2 so we can set buffer size before binding
    let addr: std::net::SocketAddr = bind_addr.parse().map_err(|e| {
        Error::ExecutionError(format!(
            "syslog: invalid bind address '{}': {}",
            bind_addr, e
        ))
    })?;
    let domain = if addr.is_ipv6() {
        socket2::Domain::IPV6
    } else {
        socket2::Domain::IPV4
    };
    let sock2 = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
        .map_err(|e| {
            Error::ExecutionError(format!("syslog: failed to create UDP socket: {}", e))
        })?;
    sock2
        .set_reuse_address(true)
        .map_err(|e| Error::ExecutionError(format!("syslog: failed to set SO_REUSEADDR: {}", e)))?;
    setsockopt_rcvbuf(&sock2, socket_receive_buffer);
    sock2
        .set_nonblocking(true)
        .map_err(|e| Error::ExecutionError(format!("syslog: failed to set non-blocking: {}", e)))?;
    sock2.bind(&addr.into()).map_err(|e| {
        Error::ExecutionError(format!(
            "syslog: failed to bind UDP socket on {}: {}",
            bind_addr, e
        ))
    })?;
    let socket = tokio::net::UdpSocket::from_std(sock2.into()).map_err(|e| {
        Error::ExecutionError(format!("syslog: failed to convert socket to tokio: {}", e))
    })?;

    info!(address = %bind_addr, "syslog UDP listener started");

    let mut buf = vec![0u8; state.max_message_size];

    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            debug!("syslog UDP listener shutting down");
            return Ok(());
        }

        let recv_result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            socket.recv_from(&mut buf),
        )
        .await;

        match recv_result {
            Ok(Ok((len, addr))) => {
                if !is_allowed(&addr.ip(), &state.allowed_cidrs) {
                    warn!(source = %addr, "syslog: rejected UDP datagram from non-allowed IP");
                    continue;
                }

                let raw = &buf[..len];
                let source_ip = addr.ip().to_string();
                let message = parse_syslog_message(raw, &source_ip);

                if let Err(e) = state.sender.try_send(Ok((message, None))) {
                    warn!(error = %e, "syslog: channel full, dropping UDP message");
                }
            }
            Ok(Err(e)) => {
                warn!(error = %e, "syslog: UDP recv_from error");
            }
            Err(_) => {
                // Timeout — loop back to check shutdown flag
            }
        }
    }
}

/// Try to set the SO_RCVBUF socket option using socket2.
fn setsockopt_rcvbuf(socket: &socket2::Socket, size: usize) {
    if let Err(e) = socket.set_recv_buffer_size(size) {
        warn!(error = %e, requested = size, "syslog: failed to set SO_RCVBUF");
    } else {
        debug!(size = size, "syslog: set SO_RCVBUF");
    }
}

/// TCP listener task — accepts connections and reads framed syslog messages.
async fn tcp_listener_task(bind_addr: &str, state: &Arc<ListenerState>) -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| {
            Error::ExecutionError(format!(
                "syslog: failed to bind TCP on {}: {}",
                bind_addr, e
            ))
        })?;

    info!(address = %bind_addr, "syslog TCP listener started");

    let active_connections = Arc::new(AtomicUsize::new(0));

    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            debug!("syslog TCP listener shutting down");
            return Ok(());
        }

        let accept_result =
            tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, addr))) => {
                if !is_allowed(&addr.ip(), &state.allowed_cidrs) {
                    warn!(source = %addr, "syslog: rejected TCP connection from non-allowed IP");
                    continue;
                }

                let current = active_connections.fetch_add(1, Ordering::SeqCst);
                if current >= state.max_connections {
                    active_connections.fetch_sub(1, Ordering::SeqCst);
                    warn!(
                        source = %addr,
                        max = state.max_connections,
                        "syslog: max TCP connections reached, rejecting"
                    );
                    continue;
                }

                debug!(source = %addr, active = current + 1, "syslog: accepted TCP connection");

                let state = Arc::clone(state);
                let active_connections = Arc::clone(&active_connections);
                let source_ip = addr.ip().to_string();

                tokio::spawn(async move {
                    handle_tcp_connection(stream, &source_ip, &state).await;
                    active_connections.fetch_sub(1, Ordering::SeqCst);
                    debug!(source = %source_ip, "syslog: TCP connection closed");
                });
            }
            Ok(Err(e)) => {
                warn!(error = %e, "syslog: TCP accept error");
            }
            Err(_) => {
                // Timeout — loop back
            }
        }
    }
}

/// Handle a single TCP connection — read framed messages until EOF or timeout.
async fn handle_tcp_connection(
    stream: tokio::net::TcpStream,
    source_ip: &str,
    state: &ListenerState,
) {
    let mut reader = BufReader::new(stream);
    let mut framing = state.framing;
    let timeout_duration = std::time::Duration::from_secs(state.connection_timeout);

    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            return;
        }

        let frame_result = tokio::time::timeout(
            timeout_duration,
            read_syslog_frame(&mut reader, &mut framing, state.max_message_size),
        )
        .await;

        match frame_result {
            Ok(Ok(Some(data))) => {
                if data.is_empty() {
                    continue;
                }
                let message = parse_syslog_message(&data, source_ip);
                if let Err(e) = state.sender.try_send(Ok((message, None))) {
                    warn!(error = %e, source = source_ip, "syslog: channel full, dropping TCP message");
                }
            }
            Ok(Ok(None)) => {
                // EOF
                return;
            }
            Ok(Err(e)) => {
                warn!(error = %e, source = source_ip, "syslog: TCP framing error");
                return;
            }
            Err(_) => {
                warn!(
                    source = source_ip,
                    timeout = state.connection_timeout,
                    "syslog: TCP connection timed out"
                );
                return;
            }
        }
    }
}

/// TLS listener task — accepts TLS connections and reads framed syslog messages.
async fn tls_listener_task(
    bind_addr: &str,
    state: &Arc<ListenerState>,
    tls_config: Option<&ServerTlsConfig>,
) -> Result<(), Error> {
    use crate::modules::tls;

    let tls_config = tls_config.ok_or_else(|| {
        Error::ConfigFailedValidation("tls configuration is required for TLS transport".into())
    })?;

    let mut server_config = tls::build_server_config(tls_config)?;
    server_config.alpn_protocols = vec![b"syslog".to_vec()];

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| {
            Error::ExecutionError(format!(
                "syslog: failed to bind TLS on {}: {}",
                bind_addr, e
            ))
        })?;

    info!(address = %bind_addr, "syslog TLS listener started");

    let active_connections = Arc::new(AtomicUsize::new(0));

    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            debug!("syslog TLS listener shutting down");
            return Ok(());
        }

        let accept_result =
            tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept()).await;

        match accept_result {
            Ok(Ok((stream, addr))) => {
                if !is_allowed(&addr.ip(), &state.allowed_cidrs) {
                    warn!(source = %addr, "syslog: rejected TLS connection from non-allowed IP");
                    continue;
                }

                let current = active_connections.fetch_add(1, Ordering::SeqCst);
                if current >= state.max_connections {
                    active_connections.fetch_sub(1, Ordering::SeqCst);
                    warn!(
                        source = %addr,
                        max = state.max_connections,
                        "syslog: max TLS connections reached, rejecting"
                    );
                    continue;
                }

                debug!(source = %addr, active = current + 1, "syslog: accepted TLS connection");

                let tls_acceptor = tls_acceptor.clone();
                let state = Arc::clone(state);
                let active_connections = Arc::clone(&active_connections);
                let source_ip = addr.ip().to_string();

                tokio::spawn(async move {
                    let timeout_duration = std::time::Duration::from_secs(state.connection_timeout);
                    match tokio::time::timeout(timeout_duration, tls_acceptor.accept(stream)).await
                    {
                        Ok(Ok(tls_stream)) => {
                            handle_tls_connection(tls_stream, &source_ip, &state).await;
                        }
                        Ok(Err(e)) => {
                            warn!(source = %source_ip, error = %e, "syslog: TLS handshake failed");
                        }
                        Err(_) => {
                            warn!(source = %source_ip, "syslog: TLS handshake timed out");
                        }
                    }
                    active_connections.fetch_sub(1, Ordering::SeqCst);
                    debug!(source = %source_ip, "syslog: TLS connection closed");
                });
            }
            Ok(Err(e)) => {
                warn!(error = %e, "syslog: TLS accept error");
            }
            Err(_) => {
                // Timeout — loop back
            }
        }
    }
}

/// Handle a single TLS connection — read framed messages until EOF or timeout.
async fn handle_tls_connection(
    stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    source_ip: &str,
    state: &ListenerState,
) {
    let mut reader = BufReader::new(stream);
    let mut framing = state.framing;
    let timeout_duration = std::time::Duration::from_secs(state.connection_timeout);

    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            return;
        }

        let frame_result = tokio::time::timeout(
            timeout_duration,
            read_syslog_frame(&mut reader, &mut framing, state.max_message_size),
        )
        .await;

        match frame_result {
            Ok(Ok(Some(data))) => {
                if data.is_empty() {
                    continue;
                }
                let message = parse_syslog_message(&data, source_ip);
                if let Err(e) = state.sender.try_send(Ok((message, None))) {
                    warn!(error = %e, source = source_ip, "syslog: channel full, dropping TLS message");
                }
            }
            Ok(Ok(None)) => {
                return;
            }
            Ok(Err(e)) => {
                warn!(error = %e, source = source_ip, "syslog: TLS framing error");
                return;
            }
            Err(_) => {
                warn!(
                    source = source_ip,
                    timeout = state.connection_timeout,
                    "syslog: TLS connection timed out"
                );
                return;
            }
        }
    }
}

#[fiddler_registration_func]
fn create_syslog(conf: Value) -> Result<ExecutionType, Error> {
    let config: SyslogConfig = serde_yaml::from_value(conf)?;

    // Validate transport
    match config.transport.as_str() {
        "udp" | "tcp" | "tls" => {}
        other => {
            return Err(Error::ConfigFailedValidation(format!(
                "invalid transport '{}': must be 'udp', 'tcp', or 'tls'",
                other
            )));
        }
    }

    // Validate framing
    let framing = Framing::from_str(&config.framing)?;

    // Validate TLS requirements
    if config.transport == "tls" && config.tls.is_none() {
        return Err(Error::ConfigFailedValidation(
            "tls configuration is required when transport is 'tls'".into(),
        ));
    }

    // Validate TLS client_auth value if TLS config is provided
    if let Some(ref tls) = config.tls {
        match tls.client_auth.as_str() {
            "none" | "optional" | "required" => {}
            other => {
                return Err(Error::ConfigFailedValidation(format!(
                    "invalid tls.client_auth '{}': must be 'none', 'optional', or 'required'",
                    other
                )));
            }
        }
        if tls.client_auth != "none" && tls.ca.is_none() {
            return Err(Error::ConfigFailedValidation(
                "tls.ca is required when tls.client_auth is not 'none'".into(),
            ));
        }
    }

    // Validate max_message_size range
    if config.max_message_size < 256 || config.max_message_size > 16_777_216 {
        return Err(Error::ConfigFailedValidation(format!(
            "max_message_size {} out of range [256, 16777216]",
            config.max_message_size
        )));
    }

    // Validate max_connections
    if config.max_connections < 1 {
        return Err(Error::ConfigFailedValidation(
            "max_connections must be at least 1".into(),
        ));
    }

    // Validate connection_timeout
    if config.connection_timeout < 1 {
        return Err(Error::ConfigFailedValidation(
            "connection_timeout must be at least 1 second".into(),
        ));
    }

    // Parse and validate CIDR allowlist
    let mut allowed_cidrs = Vec::new();
    for cidr_str in &config.allow_cidrs {
        let net: IpNet = cidr_str.parse().map_err(|e| {
            Error::ConfigFailedValidation(format!("invalid CIDR '{}': {}", cidr_str, e))
        })?;
        allowed_cidrs.push(net);
    }

    Ok(ExecutionType::Input(Box::new(SyslogInput::new(
        config,
        framing,
        allowed_cidrs,
    )?)))
}

/// Registers the syslog input plugin.
pub(crate) fn register_syslog() -> Result<(), Error> {
    let config = r#"type: object
properties:
  address:
    type: string
    default: "0.0.0.0"
    description: "Bind address"
  port:
    type: integer
    default: 1514
    description: "Listen port"
  transport:
    type: string
    default: "udp"
    enum: ["udp", "tcp", "tls"]
    description: "Transport protocol"
  framing:
    type: string
    default: "auto"
    enum: ["auto", "octet_counting", "newline"]
    description: "TCP framing method"
  max_connections:
    type: integer
    default: 512
    description: "Maximum concurrent TCP/TLS connections"
  connection_timeout:
    type: integer
    default: 60
    description: "Idle connection timeout in seconds"
  allow_cidrs:
    type: array
    items:
      type: string
    default: []
    description: "IP/CIDR allowlist"
  tls:
    type: object
    properties:
      cert:
        type: string
        description: "Server certificate — file path or inline PEM"
      key:
        type: string
        description: "Server private key — file path or inline PEM"
      ca:
        type: string
        description: "CA certificate for client verification — file path or inline PEM"
      client_auth:
        type: string
        default: "none"
        enum: ["none", "optional", "required"]
        description: "Client authentication mode"
    required:
      - cert
      - key
    description: "TLS configuration (required for tls transport)"
  max_message_size:
    type: integer
    default: 65536
    description: "Maximum syslog message size in bytes"
  socket_receive_buffer:
    type: integer
    default: 4194304
    description: "UDP socket receive buffer size hint"
  channel_buffer_size:
    type: integer
    default: 10000
    description: "Internal channel capacity"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("syslog".into(), ItemType::Input, conf_spec, create_syslog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_address(), "0.0.0.0");
        assert_eq!(default_port(), 1514);
        assert_eq!(default_transport(), "udp");
        assert_eq!(default_framing(), "auto");
        assert_eq!(default_max_connections(), 512);
        assert_eq!(default_connection_timeout(), 60);
        assert_eq!(default_max_message_size(), 65536);
        assert_eq!(default_socket_receive_buffer(), 4 * 1024 * 1024);
        assert_eq!(default_channel_buffer_size(), 10000);
    }

    #[test]
    fn test_config_deserialization_full() {
        let yaml = r#"
address: "127.0.0.1"
port: 5514
transport: "tcp"
framing: "octet_counting"
max_connections: 256
connection_timeout: 120
allow_cidrs:
  - "10.0.0.0/8"
  - "192.168.1.0/24"
tls:
  cert: "/etc/ssl/cert.pem"
  key: "/etc/ssl/key.pem"
  ca: "/etc/ssl/ca.pem"
  client_auth: "required"
max_message_size: 131072
socket_receive_buffer: 8388608
channel_buffer_size: 5000
"#;
        let config: SyslogConfig = serde_yaml::from_str(yaml).expect("failed to parse config");
        assert_eq!(config.address, "127.0.0.1");
        assert_eq!(config.port, 5514);
        assert_eq!(config.transport, "tcp");
        assert_eq!(config.framing, "octet_counting");
        assert_eq!(config.max_connections, 256);
        assert_eq!(config.connection_timeout, 120);
        assert_eq!(config.allow_cidrs.len(), 2);
        let tls = config.tls.as_ref().expect("tls config");
        assert_eq!(tls.cert, "/etc/ssl/cert.pem");
        assert_eq!(tls.key, "/etc/ssl/key.pem");
        assert_eq!(tls.ca.as_deref(), Some("/etc/ssl/ca.pem"));
        assert_eq!(tls.client_auth, "required");
        assert_eq!(config.max_message_size, 131072);
        assert_eq!(config.socket_receive_buffer, 8388608);
        assert_eq!(config.channel_buffer_size, 5000);
    }

    #[test]
    fn test_config_deserialization_defaults() {
        let yaml = "{}";
        let config: SyslogConfig = serde_yaml::from_str(yaml).expect("failed to parse config");
        assert_eq!(config.address, "0.0.0.0");
        assert_eq!(config.port, 1514);
        assert_eq!(config.transport, "udp");
        assert_eq!(config.framing, "auto");
        assert_eq!(config.max_connections, 512);
        assert_eq!(config.connection_timeout, 60);
        assert!(config.allow_cidrs.is_empty());
        assert!(config.tls.is_none());
        assert_eq!(config.max_message_size, 65536);
        assert_eq!(config.socket_receive_buffer, 4 * 1024 * 1024);
        assert_eq!(config.channel_buffer_size, 10000);
    }

    #[tokio::test]
    async fn test_config_validation_invalid_transport() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ transport: "websocket" }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("invalid transport"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_config_validation_invalid_framing() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ framing: "chunked" }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("invalid framing"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_config_validation_missing_tls_config() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ transport: "tls" }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("tls configuration is required"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_config_validation_invalid_cidr() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ allow_cidrs: ["not-a-cidr"] }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("invalid CIDR"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_config_validation_max_message_size_too_small() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ max_message_size: 100 }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("out of range"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_config_validation_max_message_size_too_large() {
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(r#"{ max_message_size: 33554432 }"#).expect("yaml parse");
        let result = create_syslog(yaml).await;
        let err = result.err().expect("should fail");
        assert!(
            err.to_string().contains("out of range"),
            "error was: {}",
            err
        );
    }

    #[test]
    fn test_register_syslog() {
        let result = register_syslog();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }

    #[test]
    fn test_tls_config_inline_pem() {
        let yaml = r#"
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
  ca: /etc/ssl/ca.crt
  client_auth: optional
"#;
        let config: SyslogConfig = serde_yaml::from_str(yaml).expect("parse");
        let tls = config.tls.as_ref().expect("tls config");
        assert!(tls.cert.contains("-----BEGIN CERTIFICATE-----"));
        assert!(tls.key.contains("-----BEGIN PRIVATE KEY-----"));
        assert_eq!(tls.ca.as_deref(), Some("/etc/ssl/ca.crt"));
        assert_eq!(tls.client_auth, "optional");
    }

    #[test]
    fn test_parse_rfc5424_message() {
        let raw = b"<165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 - Test message body";
        let msg = parse_syslog_message(raw, "10.0.0.1");

        assert_eq!(String::from_utf8_lossy(&msg.bytes), "Test message body");

        assert_eq!(
            msg.metadata.get("syslog_format"),
            Some(&Value::String("rfc5424".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_hostname"),
            Some(&Value::String("myhost".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_appname"),
            Some(&Value::String("myapp".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_procid"),
            Some(&Value::String("1234".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_msgid"),
            Some(&Value::String("ID47".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_source_ip"),
            Some(&Value::String("10.0.0.1".into()))
        );
        assert!(msg.metadata.contains_key("syslog_facility"));
        assert!(msg.metadata.contains_key("syslog_severity"));
        assert!(msg.metadata.contains_key("syslog_raw"));
    }

    #[test]
    fn test_parse_rfc5424_with_structured_data() {
        let raw = b"<165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 [exampleSDID@32473 iut=\"3\" eventSource=\"Application\" eventID=\"1011\"] Test with SD";
        let msg = parse_syslog_message(raw, "10.0.0.1");

        assert_eq!(String::from_utf8_lossy(&msg.bytes), "Test with SD");
        assert!(msg.metadata.contains_key("syslog_structured_data"));
    }

    #[test]
    fn test_parse_rfc3164_message() {
        let raw = b"<34>Oct 11 22:14:15 mymachine sshd[1234]: Failed password for invalid user";
        let msg = parse_syslog_message(raw, "192.168.1.1");

        // RFC 3164 format
        assert_eq!(
            msg.metadata.get("syslog_format"),
            Some(&Value::String("rfc3164".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_source_ip"),
            Some(&Value::String("192.168.1.1".into()))
        );
        assert!(msg.metadata.contains_key("syslog_facility"));
        assert!(msg.metadata.contains_key("syslog_severity"));
        assert!(!msg.bytes.is_empty());
    }

    #[test]
    fn test_metadata_fields_populated() {
        let raw = b"<165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 - Hello";
        let msg = parse_syslog_message(raw, "10.0.0.5");

        // Check facility: 165 / 8 = 20 (local4), 165 % 8 = 5 (notice)
        assert_eq!(
            msg.metadata.get("syslog_facility"),
            Some(&Value::String("local4".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_facility_code"),
            Some(&Value::Number(serde_yaml::Number::from(20u64)))
        );
        assert_eq!(
            msg.metadata.get("syslog_severity"),
            Some(&Value::String("notice".into()))
        );
        assert_eq!(
            msg.metadata.get("syslog_severity_code"),
            Some(&Value::Number(serde_yaml::Number::from(5u64)))
        );
        assert_eq!(
            msg.metadata.get("syslog_version"),
            Some(&Value::Number(serde_yaml::Number::from(1u64)))
        );
        assert!(msg.metadata.contains_key("syslog_timestamp"));
    }

    #[test]
    fn test_nil_values_omitted() {
        let raw = b"<165>1 2024-01-15T10:30:00Z - - - - - Just a message";
        let msg = parse_syslog_message(raw, "10.0.0.1");

        assert!(!msg.metadata.contains_key("syslog_hostname"));
        assert!(!msg.metadata.contains_key("syslog_appname"));
        assert!(!msg.metadata.contains_key("syslog_procid"));
        assert!(!msg.metadata.contains_key("syslog_msgid"));
    }

    #[tokio::test]
    async fn test_tcp_framing_octet_counting_valid() {
        let data = b"11 hello world";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::OctetCounting;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        let frame = result.expect("should succeed").expect("should have data");
        assert_eq!(frame, b"hello world");
    }

    #[tokio::test]
    async fn test_tcp_framing_octet_counting_oversized() {
        let data = b"99999 hello";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::OctetCounting;

        let result = read_syslog_frame(&mut reader, &mut framing, 100).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("exceeds max_message_size"),
            "error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_tcp_framing_octet_counting_too_many_digits() {
        let data = b"12345678 hello";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::OctetCounting;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exceeds"), "error was: {}", err);
    }

    #[tokio::test]
    async fn test_tcp_framing_newline() {
        let data = b"<34>Oct 11 22:14:15 myhost sshd: test\n";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::Newline;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        let frame = result.expect("should succeed").expect("should have data");
        assert_eq!(frame, b"<34>Oct 11 22:14:15 myhost sshd: test");
    }

    #[tokio::test]
    async fn test_tcp_framing_auto_detect_octet() {
        let data = b"5 hello";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::Auto;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        let frame = result.expect("should succeed").expect("should have data");
        assert_eq!(frame, b"hello");
        assert_eq!(framing, Framing::OctetCounting);
    }

    #[tokio::test]
    async fn test_tcp_framing_auto_detect_newline() {
        let data = b"<34>test message\n";
        let mut reader = BufReader::new(&data[..]);
        let mut framing = Framing::Auto;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        let frame = result.expect("should succeed").expect("should have data");
        assert_eq!(frame, b"<34>test message");
        assert_eq!(framing, Framing::Newline);
    }

    #[tokio::test]
    async fn test_tcp_framing_eof() {
        let data: &[u8] = b"";
        let mut reader = BufReader::new(data);
        let mut framing = Framing::Newline;

        let result = read_syslog_frame(&mut reader, &mut framing, 65536).await;
        assert!(result.expect("should succeed").is_none());
    }

    #[test]
    fn test_ip_allowlist_empty_allows_all() {
        let allowed: Vec<IpNet> = vec![];
        let ip: IpAddr = "192.168.1.100".parse().expect("valid ip");
        assert!(is_allowed(&ip, &allowed));
    }

    #[test]
    fn test_ip_allowlist_matching() {
        let allowed: Vec<IpNet> = vec!["10.0.0.0/8".parse().expect("valid cidr")];
        let ip: IpAddr = "10.1.2.3".parse().expect("valid ip");
        assert!(is_allowed(&ip, &allowed));
    }

    #[test]
    fn test_ip_allowlist_not_matching() {
        let allowed: Vec<IpNet> = vec!["10.0.0.0/8".parse().expect("valid cidr")];
        let ip: IpAddr = "192.168.1.1".parse().expect("valid ip");
        assert!(!is_allowed(&ip, &allowed));
    }

    #[test]
    fn test_strip_control_chars() {
        assert_eq!(strip_control_chars("hello\x00world"), "helloworld");
        assert_eq!(strip_control_chars("hello\tworld"), "hello\tworld");
        assert_eq!(strip_control_chars("hello\nworld"), "hello\nworld");
        assert_eq!(strip_control_chars("clean"), "clean");
    }

    #[test]
    fn test_framing_from_str() {
        assert_eq!(Framing::from_str("auto").expect("valid"), Framing::Auto);
        assert_eq!(
            Framing::from_str("octet_counting").expect("valid"),
            Framing::OctetCounting
        );
        assert_eq!(
            Framing::from_str("newline").expect("valid"),
            Framing::Newline
        );
        assert!(Framing::from_str("invalid").is_err());
    }

    #[tokio::test]
    async fn test_udp_round_trip() {
        let port = 19514u16;
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(&format!(r#"{{ port: {}, transport: "udp" }}"#, port))
                .expect("yaml parse");

        let config: SyslogConfig = serde_yaml::from_value(yaml.clone()).expect("config parse");
        let framing = Framing::from_str(&config.framing).expect("valid framing");
        let mut input = SyslogInput::new(config, framing, vec![]).expect("create input");

        // Give the listener time to bind
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Send a test syslog message via UDP
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind client");
        let test_msg = "<165>1 2024-01-15T10:30:00Z testhost testapp 1234 ID47 - UDP test message";
        socket
            .send_to(test_msg.as_bytes(), format!("127.0.0.1:{}", port))
            .await
            .expect("send");

        // Read the message
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), input.read())
            .await
            .expect("timeout")
            .expect("read");

        assert_eq!(String::from_utf8_lossy(&result.0.bytes), "UDP test message");
        assert_eq!(
            result.0.metadata.get("syslog_hostname"),
            Some(&Value::String("testhost".into()))
        );
        assert_eq!(
            result.0.metadata.get("syslog_appname"),
            Some(&Value::String("testapp".into()))
        );

        input.close().await.expect("close");
    }

    #[tokio::test]
    async fn test_tcp_round_trip() {
        let port = 19515u16;
        let config: SyslogConfig = serde_yaml::from_str(&format!(
            r#"port: {}
transport: "tcp"
framing: "newline""#,
            port
        ))
        .expect("config parse");

        let framing = Framing::from_str(&config.framing).expect("valid framing");
        let mut input = SyslogInput::new(config, framing, vec![]).expect("create input");

        // Give the listener time to bind
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect via TCP and send a syslog message
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("tcp connect");

        use tokio::io::AsyncWriteExt;
        let test_msg = "<34>Oct 11 22:14:15 tcphost sshd[5678]: TCP test message\n";
        stream.write_all(test_msg.as_bytes()).await.expect("write");

        // Read the message
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), input.read())
            .await
            .expect("timeout")
            .expect("read");

        assert!(
            String::from_utf8_lossy(&result.0.bytes).contains("TCP test message"),
            "bytes were: {}",
            String::from_utf8_lossy(&result.0.bytes)
        );
        assert_eq!(
            result.0.metadata.get("syslog_format"),
            Some(&Value::String("rfc3164".into()))
        );

        input.close().await.expect("close");
    }

    #[test]
    fn test_facility_code_values() {
        assert_eq!(facility_code(&syslog_loose::SyslogFacility::LOG_KERN), 0);
        assert_eq!(facility_code(&syslog_loose::SyslogFacility::LOG_USER), 1);
        assert_eq!(facility_code(&syslog_loose::SyslogFacility::LOG_LOCAL0), 16);
        assert_eq!(facility_code(&syslog_loose::SyslogFacility::LOG_LOCAL7), 23);
    }

    #[test]
    fn test_severity_code_values() {
        assert_eq!(severity_code(&syslog_loose::SyslogSeverity::SEV_EMERG), 0);
        assert_eq!(severity_code(&syslog_loose::SyslogSeverity::SEV_ERR), 3);
        assert_eq!(severity_code(&syslog_loose::SyslogSeverity::SEV_INFO), 6);
        assert_eq!(severity_code(&syslog_loose::SyslogSeverity::SEV_DEBUG), 7);
    }

    #[test]
    fn test_parse_message_with_bom() {
        let raw = "<165>1 2024-01-15T10:30:00Z myhost myapp 1234 ID47 - \u{FEFF}BOM message";
        let msg = parse_syslog_message(raw.as_bytes(), "10.0.0.1");
        assert_eq!(String::from_utf8_lossy(&msg.bytes), "BOM message");
    }
}
