use crate::runtime::{InternalMessage, MessageHandle, MessageStatus};
use crate::{Error, Input, InputBatch, MessageType};
use flume::{Receiver, Sender};
use tokio::time::{sleep, Duration};
use tracing::{debug, trace};
use uuid::Uuid;

#[cfg(feature = "amqp")]
pub mod amqp;
pub mod file;
#[cfg(feature = "http_server")]
pub mod http_server;
#[cfg(feature = "mqtt")]
pub mod mqtt;
#[cfg(feature = "redis")]
pub mod redis;
pub mod stdin;
#[cfg(feature = "zeromq")]
pub mod zeromq;

/// Minimum backoff duration when no input is available (in microseconds)
const NO_INPUT_BACKOFF_MIN_US: u64 = 1;

/// Maximum backoff duration when no input is available (in milliseconds)
const NO_INPUT_BACKOFF_MAX_MS: u64 = 10;

pub(crate) fn register_plugins() -> Result<(), Error> {
    file::register_file()?;
    #[cfg(feature = "http_server")]
    http_server::register_http_server()?;
    #[cfg(feature = "redis")]
    redis::register_redis()?;
    #[cfg(feature = "mqtt")]
    mqtt::register_mqtt()?;
    #[cfg(feature = "zeromq")]
    zeromq::register_zeromq()?;
    #[cfg(feature = "amqp")]
    amqp::register_amqp()?;
    stdin::register_stdin()?;
    Ok(())
}

/// Run a single-message input, reading messages one at a time and sending them to the pipeline.
pub(crate) async fn run_input(
    mut i: Box<dyn Input + Send + Sync>,
    output: Sender<InternalMessage>,
    state_handle: Sender<MessageHandle>,
    kill_switch: Receiver<()>,
) -> Result<(), Error> {
    debug!("input connected");

    // Track consecutive no-input errors for exponential backoff
    let mut no_input_count: u32 = 0;

    loop {
        tokio::select! {
            biased;
            Ok(_) = kill_switch.recv_async() => {
                i.close().await?;
                debug!("input closed by timeout");
                return Ok(());
            },
            m = i.read() => {
                match m {
                    Ok((msg, closure)) => {
                        // Reset backoff on successful read
                        no_input_count = 0;
                        let message_type = msg.message_type.clone();

                        let message_id: String = match &message_type {
                            MessageType::Default => Uuid::new_v4().into(),
                            MessageType::BeginStream(id) => id.clone(),
                            MessageType::EndStream(id) => id.clone(),
                        };

                        trace!(message_id = message_id, message_type = format!("{:?}", message_type), "received message");

                        let is_stream = match &message_type {
                            MessageType::BeginStream(_) => true,
                            MessageType::EndStream(_) => true,
                            MessageType::Default => false,
                        };
                        let input_bytes = msg.bytes.len() as u64;
                        state_handle
                            .send_async(MessageHandle {
                                message_id: message_id.clone(),
                                closure,
                                stream_id: msg.stream_id.clone(),
                                is_stream,
                                stream_complete: matches!(&message_type, MessageType::EndStream(_)),
                                input_bytes,
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;

                        // if message.type != registration forward down the pipeline
                        if let MessageType::Default = message_type {
                            let internal_msg = InternalMessage {
                                message: msg,
                                message_id: message_id.clone(),
                                status: MessageStatus::New,
                            };

                            output
                                .send_async(internal_msg)
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                        }
                    }
                    Err(e) => match e {
                        Error::EndOfInput => {
                            i.close().await?;
                            debug!("input closed");
                            return Ok(());
                        }
                        Error::NoInputToReturn => {
                            // Exponential backoff: 1μs, 2μs, 4μs, ..., up to 10ms
                            let backoff_us = NO_INPUT_BACKOFF_MIN_US
                                .saturating_mul(1u64 << no_input_count.min(20))
                                .min(NO_INPUT_BACKOFF_MAX_MS * 1000);
                            sleep(Duration::from_micros(backoff_us)).await;
                            no_input_count = no_input_count.saturating_add(1);
                            continue;
                        }
                        _ => {
                            i.close().await?;
                            debug!("input closed");
                            tracing::error!(error = format!("{}", e), "read error from input");
                            return Err(Error::ExecutionError(format!(
                                "Received error from read: {}",
                                e
                            )));
                        }
                    },
                }
            },
        }
    }
}

/// Run a batch input, reading batches of messages and sending them to the pipeline.
///
/// Uses the stream mechanism to track batch completion:
/// - Generates a unique batch_id for each batch
/// - Sends a BeginStream message before the batch
/// - All messages in the batch share the same stream_id
/// - Sends an EndStream message after the batch with the callback
pub(crate) async fn run_input_batch(
    mut i: Box<dyn InputBatch + Send + Sync>,
    output: Sender<InternalMessage>,
    state_handle: Sender<MessageHandle>,
    kill_switch: Receiver<()>,
) -> Result<(), Error> {
    debug!("batch input connected");

    // Track consecutive no-input errors for exponential backoff
    let mut no_input_count: u32 = 0;

    loop {
        tokio::select! {
            biased;
            Ok(_) = kill_switch.recv_async() => {
                i.close().await?;
                debug!("batch input closed by timeout");
                return Ok(());
            },
            m = i.read_batch() => {
                match m {
                    Ok((batch, closure)) => {
                        // Reset backoff on successful read
                        no_input_count = 0;

                        if batch.is_empty() {
                            // Empty batch, nothing to process but still trigger callback if present
                            if let Some(chan) = closure {
                                let _ = chan.send(crate::Status::Processed);
                            }
                            continue;
                        }

                        // Generate a unique batch_id to group all messages
                        let batch_id: String = Uuid::new_v4().into();
                        trace!(batch_id = batch_id, batch_size = batch.len(), "received batch");

                        // Send BeginStream to start tracking the batch
                        state_handle
                            .send_async(MessageHandle {
                                message_id: batch_id.clone(),
                                closure: None, // Callback will be attached to EndStream
                                stream_id: None,
                                is_stream: true,
                                stream_complete: false,
                                input_bytes: 0, // Stream markers don't have bytes
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;

                        // Process each message in the batch
                        for msg in batch {
                            let message_id: String = Uuid::new_v4().into();
                            let input_bytes = msg.bytes.len() as u64;

                            trace!(
                                message_id = message_id,
                                batch_id = batch_id,
                                "processing batch message"
                            );

                            // Register message with state handler, linked to batch via stream_id
                            state_handle
                                .send_async(MessageHandle {
                                    message_id: message_id.clone(),
                                    closure: None, // Individual messages don't have callbacks
                                    stream_id: Some(batch_id.clone()),
                                    is_stream: false,
                                    stream_complete: false,
                                    input_bytes,
                                })
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;

                            // Only send Default messages to the pipeline
                            if let MessageType::Default = msg.message_type {
                                let mut internal_msg_content = msg;
                                // Set stream_id on the message so it's tracked through the pipeline
                                internal_msg_content.stream_id = Some(batch_id.clone());

                                let internal_msg = InternalMessage {
                                    message: internal_msg_content,
                                    message_id,
                                    status: MessageStatus::New,
                                };

                                output
                                    .send_async(internal_msg)
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                            }
                        }

                        // Send EndStream with the callback to complete batch tracking
                        state_handle
                            .send_async(MessageHandle {
                                message_id: batch_id.clone(),
                                closure,
                                stream_id: None,
                                is_stream: true,
                                stream_complete: true,
                                input_bytes: 0, // Stream markers don't have bytes
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                    }
                    Err(e) => match e {
                        Error::EndOfInput => {
                            i.close().await?;
                            debug!("batch input closed");
                            return Ok(());
                        }
                        Error::NoInputToReturn => {
                            // Exponential backoff: 1μs, 2μs, 4μs, ..., up to 10ms
                            let backoff_us = NO_INPUT_BACKOFF_MIN_US
                                .saturating_mul(1u64 << no_input_count.min(20))
                                .min(NO_INPUT_BACKOFF_MAX_MS * 1000);
                            sleep(Duration::from_micros(backoff_us)).await;
                            no_input_count = no_input_count.saturating_add(1);
                            continue;
                        }
                        _ => {
                            i.close().await?;
                            debug!("batch input closed");
                            tracing::error!(error = format!("{}", e), "read error from batch input");
                            return Err(Error::ExecutionError(format!(
                                "Received error from read_batch: {}",
                                e
                            )));
                        }
                    },
                }
            },
        }
    }
}
