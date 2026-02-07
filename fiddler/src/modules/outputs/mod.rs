use crate::runtime::{InternalMessage, InternalMessageState, MessageStatus};
use crate::{Error, Output, OutputBatch, SHUTDOWN_MESSAGE_ID};
use flume::{Receiver, Sender};
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
) -> Result<(), Error> {
    debug!("output connected");

    loop {
        match input.recv_async().await {
            Ok(msg) => {
                trace!("received output message");
                // Extract fields before moving message to avoid clone
                let stream_id = msg.message.stream_id.clone();
                let message_id = msg.message_id;
                let output_bytes = msg.message.bytes.len() as u64;

                match o.write(msg.message).await {
                    Ok(_) => {
                        trace!("sending message");
                        state
                            .send_async(InternalMessageState {
                                message_id,
                                status: MessageStatus::Output,
                                stream_id,
                                is_stream: false,
                                bytes: output_bytes,
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for output");
                        }
                        _ => {
                            trace!("sending state");
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
                    },
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
) -> Result<(), Error> {
    debug!("output connected");

    let batch_size = o.batch_size().await;
    let interval = o.interval().await;

    loop {
        let deadline = Instant::now() + interval;
        let mut internal_msg_batch: Vec<InternalMessage> = Vec::with_capacity(batch_size);

        // Collect messages until batch is full or timeout reached
        while internal_msg_batch.len() < batch_size {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            match timeout(remaining, input.recv_async()).await {
                Ok(Ok(msg)) => internal_msg_batch.push(msg),
                Ok(Err(_)) => {
                    // Channel disconnected - process remaining batch and exit
                    if !internal_msg_batch.is_empty() {
                        process_batch(&mut o, &state, internal_msg_batch).await?;
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
            process_batch(&mut o, &state, internal_msg_batch).await?;
        }
    }
}

/// Helper function to process a batch of messages
async fn process_batch(
    o: &mut Box<dyn OutputBatch + Send + Sync>,
    state: &Sender<InternalMessageState>,
    internal_msg_batch: Vec<InternalMessage>,
) -> Result<(), Error> {
    // Extract metadata before moving messages to avoid clones
    // Include bytes for output tracking
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

    // Move messages instead of cloning
    let msg_batch: Vec<crate::Message> =
        internal_msg_batch.into_iter().map(|i| i.message).collect();

    match o.write_batch(msg_batch).await {
        Ok(_) => {
            for (message_id, stream_id, bytes) in metadata {
                state
                    .send_async(InternalMessageState {
                        message_id,
                        status: MessageStatus::Output,
                        stream_id,
                        is_stream: false,
                        bytes,
                    })
                    .await
                    .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
            }
        }
        Err(e) => match e {
            Error::ConditionalCheckfailed => {
                debug!("conditional check failed for output");
            }
            _ => {
                for (message_id, stream_id, _bytes) in metadata {
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
        },
    }
    Ok(())
}
