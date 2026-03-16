use crate::runtime::{InternalMessage, InternalMessageState, MessageStatus};
use crate::{Error, Output, OutputBatch, SHUTDOWN_MESSAGE_ID};
use flume::{Receiver, Sender};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use tracing::{debug, error, trace};

#[cfg(feature = "amqp")]
pub mod amqp;
#[cfg(feature = "clickhouse")]
pub mod clickhouse;
pub mod drop;
#[cfg(feature = "elasticsearch")]
pub mod elasticsearch;
#[cfg(feature = "http_client")]
pub mod http;
#[cfg(feature = "mqtt")]
pub mod mqtt;
#[cfg(feature = "redis")]
pub mod redis;
pub mod stdout;
pub mod switch;
#[cfg(feature = "zeromq")]
pub mod zeromq;

pub(crate) fn register_plugins() -> Result<(), Error> {
    drop::register_drop()?;
    #[cfg(feature = "elasticsearch")]
    elasticsearch::register_elasticsearch()?;
    #[cfg(feature = "clickhouse")]
    clickhouse::register_clickhouse()?;
    #[cfg(feature = "http_client")]
    http::register_http()?;
    #[cfg(feature = "redis")]
    redis::register_redis()?;
    #[cfg(feature = "mqtt")]
    mqtt::register_mqtt()?;
    #[cfg(feature = "zeromq")]
    zeromq::register_zeromq()?;
    #[cfg(feature = "amqp")]
    amqp::register_amqp()?;
    stdout::register_stdout()?;
    switch::register_switch()?;
    Ok(())
}

pub(crate) async fn run_output(
    input: Receiver<InternalMessage>,
    state: Sender<InternalMessageState>,
    mut o: Box<dyn Output + Send + Sync>,
    retry_policy: Option<crate::RetryPolicy>,
) -> Result<(), Error> {
    debug!("output connected");

    loop {
        match input.recv_async().await {
            Ok(msg) => {
                trace!("received output message");
                let stream_id = msg.message.stream_id.clone();
                let message_id = msg.message_id;
                let output_bytes = msg.message.bytes.len() as u64;

                let max_attempts = retry_policy.as_ref().map_or(1, |r| r.max_retries + 1);
                let mut last_error = None;

                for attempt in 0..max_attempts {
                    let msg_clone = msg.message.clone();
                    match o.write(msg_clone).await {
                        Ok(_) => {
                            trace!("sending message");
                            state
                                .send_async(InternalMessageState {
                                    message_id: message_id.clone(),
                                    status: MessageStatus::Output,
                                    stream_id: stream_id.clone(),
                                    is_stream: false,
                                    bytes: output_bytes,
                                })
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                            last_error = None;
                            break;
                        }
                        Err(e) => match e {
                            Error::ConditionalCheckfailed => {
                                debug!("conditional check failed for output");
                                last_error = None;
                                break;
                            }
                            Error::UnRetryable(ref msg) => {
                                debug!(error = %msg, "unretryable output error");
                                state
                                    .send_async(InternalMessageState {
                                        message_id: message_id.clone(),
                                        status: MessageStatus::OutputError(format!("{e}")),
                                        stream_id: stream_id.clone(),
                                        is_stream: false,
                                        bytes: 0,
                                    })
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                                last_error = None;
                                break;
                            }
                            _ => {
                                if attempt + 1 < max_attempts {
                                    let wait = retry_policy
                                        .as_ref()
                                        .map_or(Duration::from_secs(1), |rp| {
                                            rp.compute_wait(attempt)
                                        });
                                    tracing::warn!(
                                        attempt = attempt + 1,
                                        max_retries = max_attempts - 1,
                                        wait_ms = wait.as_millis() as u64,
                                        error = %e,
                                        "output write failed, retrying"
                                    );
                                    state
                                        .send_async(InternalMessageState {
                                            message_id: message_id.clone(),
                                            status: MessageStatus::Retry,
                                            stream_id: stream_id.clone(),
                                            is_stream: false,
                                            bytes: 0,
                                        })
                                        .await
                                        .map_err(|e| {
                                            Error::UnableToSendToChannel(format!("{e}"))
                                        })?;
                                    tokio::time::sleep(wait).await;
                                } else {
                                    last_error = Some(e);
                                }
                            }
                        },
                    }
                }

                if let Some(e) = last_error {
                    tracing::error!(
                        attempts = max_attempts,
                        error = %e,
                        "output write failed after all retries"
                    );
                    state
                        .send_async(InternalMessageState {
                            message_id: message_id.clone(),
                            status: MessageStatus::RetriesExhausted,
                            stream_id: stream_id.clone(),
                            is_stream: false,
                            bytes: 0,
                        })
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                    state
                        .send_async(InternalMessageState {
                            message_id,
                            status: MessageStatus::OutputError(format!("{e}")),
                            stream_id,
                            is_stream: false,
                            bytes: 0,
                        })
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                }
            }
            Err(_) => {
                // Channel disconnected - clean shutdown
                o.close().await?;
                debug!("output closed");
                state
                    .send_async(InternalMessageState {
                        message_id: SHUTDOWN_MESSAGE_ID.into(),
                        status: MessageStatus::Shutdown,
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                return Ok(());
            }
        }
    }
}

pub(crate) async fn run_output_batch(
    input: Receiver<InternalMessage>,
    state: Sender<InternalMessageState>,
    mut o: Box<dyn OutputBatch + Send + Sync>,
    retry_policy: Option<crate::RetryPolicy>,
) -> Result<(), Error> {
    debug!("output connected");

    let batch_size = o.batch_size().await;
    let interval = o.interval().await;
    let max_batch_bytes = o.max_batch_bytes().await;

    loop {
        let deadline = Instant::now() + interval;
        let mut internal_msg_batch: Vec<InternalMessage> = Vec::with_capacity(batch_size);
        let mut batch_bytes: usize = 0;

        // Collect messages until batch is full, byte limit reached, or timeout
        while internal_msg_batch.len() < batch_size {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            match timeout(remaining, input.recv_async()).await {
                Ok(Ok(msg)) => {
                    let msg_bytes = msg.message.bytes.len();

                    // Estimate total serialized size including minimal JSON
                    // array framing: [] brackets (2 bytes) + commas between
                    // items (count - 1 bytes after adding this message).
                    let new_count = internal_msg_batch.len() + 1;
                    let estimated_total = batch_bytes + msg_bytes + 2 + new_count.saturating_sub(1);

                    // If adding this message would exceed the byte limit and we
                    // already have messages, flush current batch first.
                    // A single message larger than the limit is always accepted
                    // (batch of 1) so we don't drop oversized messages.
                    if max_batch_bytes > 0
                        && estimated_total > max_batch_bytes
                        && !internal_msg_batch.is_empty()
                    {
                        process_batch(&mut o, &state, internal_msg_batch, &retry_policy).await?;
                        internal_msg_batch = Vec::with_capacity(batch_size);
                        batch_bytes = 0;
                    }

                    batch_bytes += msg_bytes;
                    internal_msg_batch.push(msg);
                }
                Ok(Err(_)) => {
                    // Channel disconnected - process remaining batch and exit
                    if !internal_msg_batch.is_empty() {
                        process_batch(&mut o, &state, internal_msg_batch, &retry_policy).await?;
                    }
                    o.close().await?;
                    match state
                        .send_async(InternalMessageState {
                            message_id: SHUTDOWN_MESSAGE_ID.into(),
                            status: MessageStatus::Shutdown,
                            ..Default::default()
                        })
                        .await
                    {
                        Ok(_) => debug!("exited successfully"),
                        Err(e) => error!("unable to exit {e}"),
                    }
                    return Ok(());
                }
                Err(_) => break, // Timeout reached
            }
        }

        if !internal_msg_batch.is_empty() {
            process_batch(&mut o, &state, internal_msg_batch, &retry_policy).await?;
        }
    }
}

/// Helper function to process a batch of messages
async fn process_batch(
    o: &mut Box<dyn OutputBatch + Send + Sync>,
    state: &Sender<InternalMessageState>,
    internal_msg_batch: Vec<InternalMessage>,
    retry_policy: &Option<crate::RetryPolicy>,
) -> Result<(), Error> {
    let metadata: Vec<(String, Option<String>, u64)> = internal_msg_batch
        .iter()
        .map(|i| {
            (
                i.message_id.clone(),
                i.message.stream_id.clone(),
                i.message.bytes.len() as u64,
            )
        })
        .collect();

    let max_attempts = retry_policy.as_ref().map_or(1, |r| r.max_retries + 1);
    let mut last_error = None;

    for attempt in 0..max_attempts {
        let msg_batch: Vec<crate::Message> = internal_msg_batch
            .iter()
            .map(|i| i.message.clone())
            .collect();

        match o.write_batch(msg_batch).await {
            Ok(_) => {
                for (message_id, stream_id, bytes) in &metadata {
                    state
                        .send_async(InternalMessageState {
                            message_id: message_id.clone(),
                            status: MessageStatus::Output,
                            stream_id: stream_id.clone(),
                            is_stream: false,
                            bytes: *bytes,
                        })
                        .await
                        .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                }
                last_error = None;
                break;
            }
            Err(e) => match e {
                Error::ConditionalCheckfailed => {
                    debug!("conditional check failed for output");
                    last_error = None;
                    break;
                }
                Error::UnRetryable(ref msg) => {
                    debug!(error = %msg, "unretryable batch output error");
                    for (message_id, stream_id, _bytes) in &metadata {
                        state
                            .send_async(InternalMessageState {
                                message_id: message_id.clone(),
                                status: MessageStatus::OutputError(format!("{e}")),
                                stream_id: stream_id.clone(),
                                is_stream: false,
                                bytes: 0,
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                    }
                    last_error = None;
                    break;
                }
                _ => {
                    if attempt + 1 < max_attempts {
                        let wait = retry_policy
                            .as_ref()
                            .map_or(Duration::from_secs(1), |rp| rp.compute_wait(attempt));
                        tracing::warn!(
                            attempt = attempt + 1,
                            max_retries = max_attempts - 1,
                            batch_size = metadata.len(),
                            wait_ms = wait.as_millis() as u64,
                            error = %e,
                            "batch output write failed, retrying"
                        );
                        state
                            .send_async(InternalMessageState {
                                message_id: String::new(),
                                status: MessageStatus::Retry,
                                ..Default::default()
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                        tokio::time::sleep(wait).await;
                    } else {
                        last_error = Some(e);
                    }
                }
            },
        }
    }

    if let Some(e) = last_error {
        tracing::error!(
            attempts = max_attempts,
            batch_size = metadata.len(),
            error = %e,
            "batch output write failed after all retries"
        );
        state
            .send_async(InternalMessageState {
                message_id: String::new(),
                status: MessageStatus::RetriesExhausted,
                ..Default::default()
            })
            .await
            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
        for (message_id, stream_id, _bytes) in &metadata {
            state
                .send_async(InternalMessageState {
                    message_id: message_id.clone(),
                    status: MessageStatus::OutputError(format!("{e}")),
                    stream_id: stream_id.clone(),
                    is_stream: false,
                    bytes: 0,
                })
                .await
                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
        }
    }
    Ok(())
}
