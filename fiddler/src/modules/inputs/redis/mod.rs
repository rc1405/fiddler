//! Redis input module for consuming data.
//!
//! This module provides input implementations for reading data from Redis
//! using either blocking list operations (BLPOP/BRPOP), pub/sub subscriptions,
//! or consumer group streams (XREADGROUP).
//!
//! # Configuration
//!
//! ## List mode
//!
//! ```yaml
//! input:
//!   redis:
//!     url: "redis://localhost:6379/0"   # Required: Redis connection URL
//!     mode: "list"                       # Optional: "list", "pubsub", or "stream" (default: "list")
//!     keys:                              # Required for list mode: key names
//!       - "input_queue"
//!     list_command: "brpop"              # Optional: "blpop" or "brpop" (default: "brpop")
//!     timeout: 1                         # Optional: blocking timeout in seconds (default: 1)
//! ```
//!
//! ## Pubsub mode
//!
//! ```yaml
//! input:
//!   redis:
//!     url: "redis://localhost:6379/0"
//!     mode: "pubsub"
//!     channels:                          # Required for pubsub mode: channels/patterns
//!       - "events.*"
//!     use_patterns: false                # Optional: use pattern matching for pubsub (default: false)
//! ```
//!
//! ## Stream mode
//!
//! ```yaml
//! input:
//!   redis:
//!     url: "redis://localhost:6379"
//!     mode: stream
//!     streams:
//!       - my_stream
//!     consumer_group: my_group
//!     consumer_name: "worker-1"
//!     stream_read_count: 100
//!     block_ms: 5000
//!     auto_claim:
//!       enabled: true
//!       idle_ms: 30000
//!       interval_ms: 10000
//!       batch_size: 100
//!     create_group: true
//! ```

use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::{new_callback_chan, CallbackChan, Closer, Error, Input, Message, Status};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use futures::StreamExt;
use redis::{Client, ErrorKind};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::HashMap;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};

/// Check if a Redis error is an authentication failure.
fn is_auth_error(e: &redis::RedisError) -> bool {
    matches!(e.kind(), ErrorKind::AuthenticationFailed)
        || e.to_string().to_lowercase().contains("noauth")
        || e.to_string().to_lowercase().contains("wrongpass")
        || e.to_string().to_lowercase().contains("invalid password")
}

/// Convert a Redis error to a fiddler Error, with special handling for auth failures.
fn redis_error_to_fiddler_error(e: redis::RedisError, context: &str) -> Error {
    if is_auth_error(&e) {
        Error::ConfigFailedValidation(format!("Redis authentication failed: {}", e))
    } else {
        Error::ExecutionError(format!("{}: {}", context, e))
    }
}

const DEFAULT_MODE: &str = "list";
const DEFAULT_LIST_COMMAND: &str = "brpop";
const DEFAULT_TIMEOUT: u64 = 1;
const CHANNEL_BUFFER_SIZE: usize = 100;

fn default_true() -> bool {
    true
}

fn default_idle_ms() -> u64 {
    30_000
}

fn default_interval_ms() -> u64 {
    10_000
}

fn default_auto_claim_batch_size() -> usize {
    100
}

fn default_stream_read_count() -> usize {
    100
}

fn default_block_ms() -> u64 {
    5000
}

/// Auto-claim configuration for reclaiming idle pending messages.
#[derive(Deserialize, Clone, Debug)]
pub struct AutoClaimConfig {
    /// Whether auto-claiming is enabled (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Minimum idle time in milliseconds before a message can be claimed (default: 30000).
    #[serde(default = "default_idle_ms")]
    pub idle_ms: u64,
    /// Interval in milliseconds between auto-claim runs (default: 10000).
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    /// Maximum number of entries to claim per run (default: 100).
    #[serde(default = "default_auto_claim_batch_size")]
    pub batch_size: usize,
}

impl Default for AutoClaimConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_ms: 30_000,
            interval_ms: 10_000,
            batch_size: 100,
        }
    }
}

/// Returns the system hostname as the default consumer name.
fn default_consumer_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "fiddler-consumer".to_string())
}

/// Redis input configuration.
#[derive(Deserialize, Clone)]
pub struct RedisInputConfig {
    /// Redis connection URL (required).
    pub url: String,
    /// Operation mode: "list", "pubsub", or "stream" (default: "list").
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Key names for list mode.
    pub keys: Option<Vec<String>>,
    /// Channel names/patterns for pubsub mode.
    pub channels: Option<Vec<String>>,
    /// List command: "blpop" or "brpop" (default: "brpop").
    #[serde(default = "default_list_command")]
    pub list_command: String,
    /// Blocking timeout in seconds (default: 1).
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Use pattern matching for pubsub (default: false).
    #[serde(default)]
    pub use_patterns: bool,
    /// Stream names for stream mode.
    pub streams: Option<Vec<String>>,
    /// Consumer group name for stream mode.
    pub consumer_group: Option<String>,
    /// Consumer name for stream mode (default: hostname).
    #[serde(default = "default_consumer_name")]
    pub consumer_name: String,
    /// Max entries per XREADGROUP call (default: 100).
    #[serde(default = "default_stream_read_count")]
    pub stream_read_count: usize,
    /// XREADGROUP block timeout in milliseconds (default: 5000).
    #[serde(default = "default_block_ms")]
    pub block_ms: u64,
    /// Auto-claim configuration.
    #[serde(default)]
    pub auto_claim: AutoClaimConfig,
    /// Whether to auto-create consumer group on startup (default: true).
    #[serde(default = "default_true")]
    pub create_group: bool,
}

fn default_mode() -> String {
    DEFAULT_MODE.to_string()
}

fn default_list_command() -> String {
    DEFAULT_LIST_COMMAND.to_string()
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

/// A single entry from a Redis stream.
struct StreamEntry {
    /// The entry ID (e.g. "1234567890-0").
    id: String,
    /// The field-value pairs in the entry.
    fields: HashMap<String, Vec<u8>>,
}

/// Result of reading from a single stream via XREADGROUP.
struct StreamReadResult {
    /// The stream name.
    stream: String,
    /// The entries read.
    entries: Vec<StreamEntry>,
}

impl redis::FromRedisValue for StreamReadResult {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        match v {
            redis::Value::Array(arr) if arr.len() == 2 => {
                let stream: String = redis::FromRedisValue::from_redis_value(&arr[0])?;
                let entries = parse_stream_entries(&arr[1])?;
                Ok(StreamReadResult { stream, entries })
            }
            _ => Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Expected stream read result array",
            ))),
        }
    }
}

/// Parse stream entries from a Redis response value.
///
/// Expects the format: `[[id, [field, value, field, value, ...]], ...]`
fn parse_stream_entries(v: &redis::Value) -> redis::RedisResult<Vec<StreamEntry>> {
    let entries_arr = match v {
        redis::Value::Array(arr) => arr,
        _ => {
            return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Expected array of stream entries",
            )))
        }
    };

    let mut entries = Vec::with_capacity(entries_arr.len());
    for entry_val in entries_arr {
        let entry_arr = match entry_val {
            redis::Value::Array(arr) if arr.len() == 2 => arr,
            _ => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Expected [id, [fields...]] entry",
                )))
            }
        };

        let id: String = redis::FromRedisValue::from_redis_value(&entry_arr[0])?;
        let fields_arr = match &entry_arr[1] {
            redis::Value::Array(arr) => arr,
            _ => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Expected array of field-value pairs",
                )))
            }
        };

        let mut fields = HashMap::new();
        for chunk in fields_arr.chunks(2) {
            if chunk.len() == 2 {
                let key: String = redis::FromRedisValue::from_redis_value(&chunk[0])?;
                let val: Vec<u8> = redis::FromRedisValue::from_redis_value(&chunk[1])?;
                fields.insert(key, val);
            }
        }

        entries.push(StreamEntry { id, fields });
    }
    Ok(entries)
}

/// Send a stream entry through the pipeline channel with an XACK callback.
async fn send_stream_entry(
    entry: &StreamEntry,
    stream_name: &str,
    group_name: &str,
    conn: &redis::aio::MultiplexedConnection,
    sender: &Sender<Result<(Message, Option<CallbackChan>), Error>>,
) -> bool {
    // Build message bytes: prefer "body" field, otherwise JSON-encode all fields
    let bytes = if let Some(body) = entry.fields.get("body") {
        body.clone()
    } else {
        // Encode all fields as JSON
        let string_fields: HashMap<&String, String> = entry
            .fields
            .iter()
            .map(|(k, v)| (k, String::from_utf8_lossy(v).to_string()))
            .collect();
        serde_json::to_vec(&string_fields).unwrap_or_default()
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "redis_stream_id".to_string(),
        Value::String(entry.id.clone()),
    );
    metadata.insert(
        "redis_stream".to_string(),
        Value::String(stream_name.to_string()),
    );

    let msg = Message {
        bytes,
        metadata,
        ..Default::default()
    };

    let (tx, rx) = new_callback_chan();

    let mut ack_conn = conn.clone();
    let ack_stream = stream_name.to_string();
    let ack_group = group_name.to_string();
    let ack_id = entry.id.clone();

    tokio::spawn(async move {
        if let Ok(Status::Processed) = rx.await {
            let result: Result<(), redis::RedisError> = redis::cmd("XACK")
                .arg(&ack_stream)
                .arg(&ack_group)
                .arg(&ack_id)
                .query_async(&mut ack_conn)
                .await;
            if let Err(e) = result {
                warn!(error = %e, stream = %ack_stream, id = %ack_id, "Failed to XACK message");
            }
        }
    });

    sender.send_async(Ok((msg, Some(tx)))).await.is_ok()
}

/// Check if a shutdown signal has been received.
fn is_shutdown(shutdown_rx: &mut oneshot::Receiver<()>) -> bool {
    match shutdown_rx.try_recv() {
        Ok(_) | Err(oneshot::error::TryRecvError::Closed) => true,
        Err(oneshot::error::TryRecvError::Empty) => false,
    }
}

/// Process pending messages (entries that were delivered but not yet acknowledged).
async fn process_pending(
    conn: &mut redis::aio::MultiplexedConnection,
    streams: &[String],
    group: &str,
    consumer: &str,
    count: usize,
    sender: &Sender<Result<(Message, Option<CallbackChan>), Error>>,
    shutdown_rx: &mut oneshot::Receiver<()>,
) {
    for stream in streams {
        let mut last_id = "0".to_string();
        loop {
            if is_shutdown(shutdown_rx) {
                return;
            }

            let result: Result<Vec<StreamReadResult>, redis::RedisError> = redis::cmd("XREADGROUP")
                .arg("GROUP")
                .arg(group)
                .arg(consumer)
                .arg("COUNT")
                .arg(count)
                .arg("STREAMS")
                .arg(stream)
                .arg(&last_id)
                .query_async(conn)
                .await;

            match result {
                Ok(results) => {
                    let mut found_entries = false;
                    for sr in &results {
                        for entry in &sr.entries {
                            found_entries = true;
                            last_id.clone_from(&entry.id);
                            if !send_stream_entry(entry, stream, group, conn, sender).await {
                                return;
                            }
                        }
                    }
                    if !found_entries {
                        break; // No more pending entries for this stream
                    }
                }
                Err(e) => {
                    warn!(error = %e, stream = %stream, "Failed to read pending entries");
                    break;
                }
            }
        }
    }
}

/// Background task for reading from Redis streams using XREADGROUP with consumer groups.
async fn stream_reader_task(
    config: RedisInputConfig,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let streams = config.streams.unwrap_or_default();
    let group = config
        .consumer_group
        .unwrap_or_else(|| "fiddler".to_string());
    let consumer = config.consumer_name;
    let count = config.stream_read_count;
    let block_ms = config.block_ms;

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to open Redis client",
                )))
                .await;
            return;
        }
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to connect to Redis",
                )))
                .await;
            return;
        }
    };

    // Create consumer groups if configured
    if config.create_group {
        for stream in &streams {
            let result: Result<String, redis::RedisError> = redis::cmd("XGROUP")
                .arg("CREATE")
                .arg(stream)
                .arg(&group)
                .arg("0")
                .arg("MKSTREAM")
                .query_async(&mut conn)
                .await;
            match result {
                Ok(_) => {
                    debug!(stream = %stream, group = %group, "Created consumer group");
                }
                Err(e) => {
                    // BUSYGROUP means the group already exists, which is fine
                    if !e.to_string().contains("BUSYGROUP") {
                        error!(error = %e, stream = %stream, group = %group, "Failed to create consumer group");
                        let _ = sender
                            .send_async(Err(redis_error_to_fiddler_error(
                                e,
                                "Failed to create consumer group",
                            )))
                            .await;
                        return;
                    }
                    debug!(stream = %stream, group = %group, "Consumer group already exists");
                }
            }
        }
    }

    debug!(
        streams = ?streams,
        group = %group,
        consumer = %consumer,
        "Redis stream reader started"
    );

    // Process pending messages first
    process_pending(
        &mut conn,
        &streams,
        &group,
        &consumer,
        count,
        &sender,
        &mut shutdown_rx,
    )
    .await;

    // Main read loop - read new messages with ">"
    loop {
        if is_shutdown(&mut shutdown_rx) {
            debug!("Redis stream reader received shutdown signal");
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        let mut cmd = redis::cmd("XREADGROUP");
        cmd.arg("GROUP")
            .arg(&group)
            .arg(&consumer)
            .arg("COUNT")
            .arg(count)
            .arg("BLOCK")
            .arg(block_ms)
            .arg("STREAMS");

        for stream in &streams {
            cmd.arg(stream);
        }
        for _ in &streams {
            cmd.arg(">");
        }

        let result: Result<Option<Vec<StreamReadResult>>, redis::RedisError> =
            cmd.query_async(&mut conn).await;

        match result {
            Ok(Some(results)) => {
                for sr in &results {
                    for entry in &sr.entries {
                        if !send_stream_entry(entry, &sr.stream, &group, &conn, &sender).await {
                            debug!("Redis stream reader channel closed");
                            return;
                        }
                    }
                }
            }
            Ok(None) => {
                // Block timeout, no new messages - continue loop
            }
            Err(e) => {
                if is_auth_error(&e) {
                    error!(error = %e, "Redis authentication failed");
                    let _ = sender
                        .send_async(Err(Error::ConfigFailedValidation(format!(
                            "Redis authentication failed: {}",
                            e
                        ))))
                        .await;
                    return;
                }

                error!(error = %e, "Redis XREADGROUP failed");
                match client.get_multiplexed_async_connection().await {
                    Ok(new_conn) => {
                        conn = new_conn;
                        warn!("Reconnected to Redis");
                    }
                    Err(reconnect_err) => {
                        if is_auth_error(&reconnect_err) {
                            error!(error = %reconnect_err, "Redis authentication failed on reconnect");
                            let _ = sender
                                .send_async(Err(Error::ConfigFailedValidation(format!(
                                    "Redis authentication failed: {}",
                                    reconnect_err
                                ))))
                                .await;
                            return;
                        }
                        error!(error = %reconnect_err, "Failed to reconnect to Redis");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}

/// Background task for auto-claiming idle pending messages using XAUTOCLAIM.
async fn auto_claim_task(
    config: RedisInputConfig,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let streams = config.streams.unwrap_or_default();
    let group = config
        .consumer_group
        .unwrap_or_else(|| "fiddler".to_string());
    let consumer = config.consumer_name;
    let auto_claim = config.auto_claim;

    if !auto_claim.enabled {
        debug!("Auto-claim disabled, task exiting");
        return;
    }

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Auto-claim: failed to open Redis client");
            return;
        }
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Auto-claim: failed to connect to Redis");
            return;
        }
    };

    let interval = std::time::Duration::from_millis(auto_claim.interval_ms);

    debug!(
        streams = ?streams,
        group = %group,
        consumer = %consumer,
        idle_ms = auto_claim.idle_ms,
        interval_ms = auto_claim.interval_ms,
        batch_size = auto_claim.batch_size,
        "Auto-claim task started"
    );

    loop {
        // Wait for the interval, checking for shutdown
        match tokio::time::timeout(interval, &mut shutdown_rx).await {
            Ok(_) => {
                debug!("Auto-claim task received shutdown signal");
                return;
            }
            Err(_) => {
                // Timeout elapsed - time to run auto-claim
            }
        }

        for stream in &streams {
            let mut cursor = "0-0".to_string();
            loop {
                let result: Result<redis::Value, redis::RedisError> = redis::cmd("XAUTOCLAIM")
                    .arg(stream)
                    .arg(&group)
                    .arg(&consumer)
                    .arg(auto_claim.idle_ms)
                    .arg(&cursor)
                    .arg("COUNT")
                    .arg(auto_claim.batch_size)
                    .query_async(&mut conn)
                    .await;

                match result {
                    Ok(redis::Value::Array(ref parts)) if parts.len() >= 2 => {
                        // Parse cursor
                        let new_cursor: String =
                            match redis::FromRedisValue::from_redis_value(&parts[0]) {
                                Ok(c) => c,
                                Err(e) => {
                                    warn!(error = %e, "Auto-claim: failed to parse cursor");
                                    break;
                                }
                            };

                        // Parse entries
                        let entries = match parse_stream_entries(&parts[1]) {
                            Ok(e) => e,
                            Err(e) => {
                                warn!(error = %e, "Auto-claim: failed to parse entries");
                                break;
                            }
                        };

                        debug!(
                            stream = %stream,
                            claimed = entries.len(),
                            cursor = %new_cursor,
                            "Auto-claimed entries"
                        );

                        for entry in &entries {
                            if !send_stream_entry(entry, stream, &group, &conn, &sender).await {
                                return;
                            }
                        }

                        if new_cursor == "0-0" {
                            break; // No more entries to claim
                        }
                        cursor = new_cursor;
                    }
                    Ok(_) => {
                        // Unexpected response format
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, stream = %stream, "Auto-claim failed");
                        // Try to reconnect
                        match client.get_multiplexed_async_connection().await {
                            Ok(new_conn) => {
                                conn = new_conn;
                                warn!("Auto-claim: reconnected to Redis");
                            }
                            Err(reconnect_err) => {
                                warn!(error = %reconnect_err, "Auto-claim: failed to reconnect");
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}

/// Background task for reading from Redis lists.
async fn list_reader_task(
    config: RedisInputConfig,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let keys = config.keys.unwrap_or_default();
    let use_blpop = config.list_command.to_lowercase() == "blpop";
    let timeout_secs = config.timeout;

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to open Redis client",
                )))
                .await;
            return;
        }
    };

    let mut conn = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to connect to Redis",
                )))
                .await;
            return;
        }
    };

    debug!(keys = ?keys, use_blpop = use_blpop, "Redis list reader started");

    loop {
        // Check for shutdown
        match shutdown_rx.try_recv() {
            Ok(_) | Err(oneshot::error::TryRecvError::Closed) => {
                debug!("Redis list reader received shutdown signal");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        // Build the command
        let result: Result<Option<(String, Vec<u8>)>, redis::RedisError> = if use_blpop {
            redis::cmd("BLPOP")
                .arg(&keys)
                .arg(timeout_secs)
                .query_async(&mut conn)
                .await
        } else {
            redis::cmd("BRPOP")
                .arg(&keys)
                .arg(timeout_secs)
                .query_async(&mut conn)
                .await
        };

        match result {
            Ok(Some((_key, data))) => {
                let msg = Message {
                    bytes: data,
                    ..Default::default()
                };
                if sender.send_async(Ok((msg, None))).await.is_err() {
                    debug!("Redis list reader channel closed");
                    return;
                }
            }
            Ok(None) => {
                // Timeout, no data available - continue loop
            }
            Err(e) => {
                // Check for authentication errors - these are fatal
                if is_auth_error(&e) {
                    error!(error = %e, "Redis authentication failed");
                    let _ = sender
                        .send_async(Err(Error::ConfigFailedValidation(format!(
                            "Redis authentication failed: {}",
                            e
                        ))))
                        .await;
                    return;
                }

                error!(error = %e, "Redis BRPOP/BLPOP failed");
                // Try to reconnect
                match client.get_multiplexed_async_connection().await {
                    Ok(new_conn) => {
                        conn = new_conn;
                        warn!("Reconnected to Redis");
                    }
                    Err(reconnect_err) => {
                        // Check if reconnect failed due to auth
                        if is_auth_error(&reconnect_err) {
                            error!(error = %reconnect_err, "Redis authentication failed on reconnect");
                            let _ = sender
                                .send_async(Err(Error::ConfigFailedValidation(format!(
                                    "Redis authentication failed: {}",
                                    reconnect_err
                                ))))
                                .await;
                            return;
                        }
                        error!(error = %reconnect_err, "Failed to reconnect to Redis");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}

/// Background task for reading from Redis pub/sub.
async fn pubsub_reader_task(
    config: RedisInputConfig,
    sender: Sender<Result<(Message, Option<CallbackChan>), Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let channels = config.channels.unwrap_or_default();
    let use_patterns = config.use_patterns;

    let client = match Client::open(config.url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to open Redis client",
                )))
                .await;
            return;
        }
    };

    let mut pubsub = match client.get_async_pubsub().await {
        Ok(ps) => ps,
        Err(e) => {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    "Failed to create pub/sub connection",
                )))
                .await;
            return;
        }
    };

    // Subscribe to channels
    for channel in &channels {
        let result = if use_patterns {
            pubsub.psubscribe(channel).await
        } else {
            pubsub.subscribe(channel).await
        };

        if let Err(e) = result {
            let _ = sender
                .send_async(Err(redis_error_to_fiddler_error(
                    e,
                    &format!("Failed to subscribe to {}", channel),
                )))
                .await;
            return;
        }
    }

    debug!(channels = ?channels, use_patterns = use_patterns, "Redis pub/sub reader started");

    loop {
        // Check for shutdown
        match shutdown_rx.try_recv() {
            Ok(_) | Err(oneshot::error::TryRecvError::Closed) => {
                debug!("Redis pub/sub reader received shutdown signal");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        // Use timeout to allow checking shutdown
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            pubsub.on_message().next(),
        )
        .await;

        match result {
            Ok(Some(msg)) => {
                let payload: Vec<u8> = match msg.get_payload_bytes() {
                    bytes => bytes.to_vec(),
                };
                let message = Message {
                    bytes: payload,
                    ..Default::default()
                };
                if sender.send_async(Ok((message, None))).await.is_err() {
                    debug!("Redis pub/sub reader channel closed");
                    return;
                }
            }
            Ok(None) => {
                // Stream ended
                debug!("Redis pub/sub stream ended");
                let _ = sender.send_async(Err(Error::EndOfInput)).await;
                return;
            }
            Err(_) => {
                // Timeout, continue loop to check shutdown
            }
        }
    }
}

/// Redis input reader.
pub struct RedisInput {
    receiver: Receiver<Result<(Message, Option<CallbackChan>), Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    auto_claim_shutdown_tx: Option<oneshot::Sender<()>>,
}

impl RedisInput {
    /// Creates a new Redis input.
    pub fn new(config: RedisInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER_SIZE);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let mode = config.mode.clone();

        match mode.as_str() {
            "list" => {
                tokio::spawn(list_reader_task(config, sender, shutdown_rx));
                Ok(Self {
                    receiver,
                    shutdown_tx: Some(shutdown_tx),
                    auto_claim_shutdown_tx: None,
                })
            }
            "pubsub" => {
                tokio::spawn(pubsub_reader_task(config, sender, shutdown_rx));
                Ok(Self {
                    receiver,
                    shutdown_tx: Some(shutdown_tx),
                    auto_claim_shutdown_tx: None,
                })
            }
            "stream" => {
                // Spawn auto-claim task if enabled
                let auto_claim_shutdown_tx = if config.auto_claim.enabled {
                    let (ac_shutdown_tx, ac_shutdown_rx) = oneshot::channel();
                    tokio::spawn(auto_claim_task(
                        config.clone(),
                        sender.clone(),
                        ac_shutdown_rx,
                    ));
                    Some(ac_shutdown_tx)
                } else {
                    None
                };

                tokio::spawn(stream_reader_task(config, sender, shutdown_rx));

                Ok(Self {
                    receiver,
                    shutdown_tx: Some(shutdown_tx),
                    auto_claim_shutdown_tx,
                })
            }
            _ => Err(Error::ConfigFailedValidation(
                "mode must be 'list', 'pubsub', or 'stream'".into(),
            )),
        }
    }
}

#[async_trait]
impl Input for RedisInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(Ok((msg, cb))) => Ok((msg, cb)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for RedisInput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.auto_claim_shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("Redis input closing");
        Ok(())
    }
}

#[fiddler_registration_func]
fn create_redis_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: RedisInputConfig = serde_yaml::from_value(conf)?;

    // Validate URL
    if config.url.is_empty() {
        return Err(Error::ConfigFailedValidation("url is required".into()));
    }

    // Validate mode-specific requirements
    match config.mode.as_str() {
        "list" => {
            if config.keys.is_none() || config.keys.as_ref().is_some_and(|k| k.is_empty()) {
                return Err(Error::ConfigFailedValidation(
                    "keys is required for list mode".into(),
                ));
            }
            if !["blpop", "brpop"].contains(&config.list_command.to_lowercase().as_str()) {
                return Err(Error::ConfigFailedValidation(
                    "list_command must be 'blpop' or 'brpop'".into(),
                ));
            }
        }
        "pubsub" => {
            if config.channels.is_none() || config.channels.as_ref().is_some_and(|c| c.is_empty()) {
                return Err(Error::ConfigFailedValidation(
                    "channels is required for pubsub mode".into(),
                ));
            }
        }
        "stream" => {
            if config.streams.is_none() || config.streams.as_ref().is_some_and(|s| s.is_empty()) {
                return Err(Error::ConfigFailedValidation(
                    "streams is required for stream mode".into(),
                ));
            }
            if config.consumer_group.is_none()
                || config.consumer_group.as_ref().is_some_and(|g| g.is_empty())
            {
                return Err(Error::ConfigFailedValidation(
                    "consumer_group is required for stream mode".into(),
                ));
            }
        }
        _ => {
            return Err(Error::ConfigFailedValidation(
                "mode must be 'list', 'pubsub', or 'stream'".into(),
            ));
        }
    }

    Ok(ExecutionType::Input(Box::new(RedisInput::new(config)?)))
}

/// Registers the Redis input plugin.
pub(crate) fn register_redis() -> Result<(), Error> {
    let config = r#"type: object
required:
  - url
properties:
  url:
    type: string
    description: "Redis connection URL (redis://host:port/db)"
  mode:
    type: string
    enum: ["list", "pubsub", "stream"]
    default: "list"
    description: "Operation mode"
  keys:
    type: array
    items:
      type: string
    description: "List key names to pop from (required for list mode)"
  channels:
    type: array
    items:
      type: string
    description: "Pub/Sub channels to subscribe (required for pubsub mode)"
  list_command:
    type: string
    enum: ["blpop", "brpop"]
    default: "brpop"
    description: "Blocking list operation (for list mode)"
  timeout:
    type: integer
    default: 1
    description: "Blocking timeout in seconds"
  use_patterns:
    type: boolean
    default: false
    description: "Use pattern matching for channels (pubsub mode)"
  streams:
    type: array
    items:
      type: string
    description: "Stream names to read from (required for stream mode)"
  consumer_group:
    type: string
    description: "Consumer group name (required for stream mode)"
  consumer_name:
    type: string
    description: "Consumer name (default: hostname)"
  stream_read_count:
    type: integer
    default: 100
    description: "Max entries per XREADGROUP call"
  block_ms:
    type: integer
    default: 5000
    description: "XREADGROUP block timeout in milliseconds"
  create_group:
    type: boolean
    default: true
    description: "Auto-create consumer group on startup"
  auto_claim:
    type: object
    description: "Auto-claim configuration for idle pending messages"
    properties:
      enabled:
        type: boolean
        default: true
        description: "Enable auto-claiming"
      idle_ms:
        type: integer
        default: 30000
        description: "Minimum idle time before claiming (ms)"
      interval_ms:
        type: integer
        default: 10000
        description: "Interval between auto-claim runs (ms)"
      batch_size:
        type: integer
        default: 100
        description: "Max entries to claim per run"
"#;
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "redis".into(),
        ItemType::Input,
        conf_spec,
        create_redis_input,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization_list() {
        let yaml = r#"
url: "redis://localhost:6379/0"
mode: "list"
keys:
  - "input_queue"
  - "fallback_queue"
list_command: "brpop"
timeout: 5
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.url, "redis://localhost:6379/0");
        assert_eq!(config.mode, "list");
        assert_eq!(
            config.keys,
            Some(vec![
                "input_queue".to_string(),
                "fallback_queue".to_string()
            ])
        );
        assert_eq!(config.list_command, "brpop");
        assert_eq!(config.timeout, 5);
    }

    #[test]
    fn test_config_deserialization_pubsub() {
        let yaml = r#"
url: "redis://localhost:6379"
mode: "pubsub"
channels:
  - "events.*"
  - "logs.*"
use_patterns: true
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.mode, "pubsub");
        assert_eq!(
            config.channels,
            Some(vec!["events.*".to_string(), "logs.*".to_string()])
        );
        assert!(config.use_patterns);
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
url: "redis://localhost:6379"
keys:
  - "test"
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.mode, "list");
        assert_eq!(config.list_command, "brpop");
        assert_eq!(config.timeout, 1);
        assert!(!config.use_patterns);
    }

    #[test]
    fn test_config_deserialization_stream() {
        let yaml = r#"
url: "redis://localhost:6379"
mode: "stream"
streams:
  - "my_stream"
  - "other_stream"
consumer_group: "my_group"
consumer_name: "worker-1"
stream_read_count: 50
block_ms: 2000
create_group: false
auto_claim:
  enabled: true
  idle_ms: 60000
  interval_ms: 5000
  batch_size: 200
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.mode, "stream");
        assert_eq!(
            config.streams,
            Some(vec!["my_stream".to_string(), "other_stream".to_string()])
        );
        assert_eq!(config.consumer_group, Some("my_group".to_string()));
        assert_eq!(config.consumer_name, "worker-1");
        assert_eq!(config.stream_read_count, 50);
        assert_eq!(config.block_ms, 2000);
        assert!(!config.create_group);
        assert!(config.auto_claim.enabled);
        assert_eq!(config.auto_claim.idle_ms, 60000);
        assert_eq!(config.auto_claim.interval_ms, 5000);
        assert_eq!(config.auto_claim.batch_size, 200);
    }

    #[test]
    fn test_config_stream_defaults() {
        let yaml = r#"
url: "redis://localhost:6379"
mode: "stream"
streams:
  - "my_stream"
consumer_group: "my_group"
"#;
        let config: RedisInputConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.stream_read_count, 100);
        assert_eq!(config.block_ms, 5000);
        assert!(config.create_group);
        assert!(!config.consumer_name.is_empty());
        // Default auto_claim values
        assert!(config.auto_claim.enabled);
        assert_eq!(config.auto_claim.idle_ms, 30_000);
        assert_eq!(config.auto_claim.interval_ms, 10_000);
        assert_eq!(config.auto_claim.batch_size, 100);
    }

    #[test]
    fn test_auto_claim_config_default() {
        let ac = AutoClaimConfig::default();
        assert!(ac.enabled);
        assert_eq!(ac.idle_ms, 30_000);
        assert_eq!(ac.interval_ms, 10_000);
        assert_eq!(ac.batch_size, 100);
    }

    #[test]
    fn test_auto_claim_config_partial() {
        let yaml = r#"
enabled: false
idle_ms: 60000
"#;
        let ac: AutoClaimConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert!(!ac.enabled);
        assert_eq!(ac.idle_ms, 60000);
        assert_eq!(ac.interval_ms, 10_000); // default
        assert_eq!(ac.batch_size, 100); // default
    }

    #[test]
    fn test_register_redis() {
        let result = register_redis();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
