use crate::Error;
pub mod drop;
#[cfg(feature = "elasticsearch")]
pub mod elasticsearch;
pub mod stdout;
pub mod switch;
use crate::runtime::{InternalMessage, InternalMessageState, MessageStatus};
use crate::{Output, OutputBatch};
use async_channel::{Receiver, Sender, TryRecvError};
use std::time;
use tokio::time::{sleep, Duration};
use tracing::{debug, trace};

pub(crate) fn register_plugins() -> Result<(), Error> {
    drop::register_drop()?;
    #[cfg(feature = "elasticsearch")]
    elasticsearch::register_elasticsearch()?;
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
        match input.try_recv() {
            Ok(msg) => {
                trace!("received output message");
                match o.write(msg.message.clone()).await {
                    Ok(_) => {
                        trace!("sending message");
                        state
                            .send(InternalMessageState {
                                message_id: msg.message_id,
                                status: MessageStatus::Output,
                            })
                            .await
                            .unwrap();
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for output");
                        }
                        _ => {
                            trace!("sending state");
                            state
                                .send(InternalMessageState {
                                    message_id: msg.message_id,
                                    status: MessageStatus::OutputError(format!("{}", e)),
                                })
                                .await
                                .unwrap();
                        }
                    },
                }
            }
            Err(e) => match e {
                TryRecvError::Closed => {
                    o.close().await?;
                    debug!("output closed");
                    state
                        .send(InternalMessageState {
                            message_id: "end of the line".into(),
                            status: MessageStatus::Shutdown,
                        })
                        .await
                        .unwrap();
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
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

    let mut now = time::SystemTime::now();

    loop {
        if input.is_closed() {
            break;
        };

        let mut internal_msg_batch: Vec<InternalMessage> = Vec::new();
        while (now.elapsed().unwrap() < interval) && (internal_msg_batch.len() < batch_size) {
            match input.try_recv() {
                Ok(i) => internal_msg_batch.push(i),
                Err(e) => match e {
                    TryRecvError::Closed => break,
                    TryRecvError::Empty => sleep(Duration::from_millis(25)).await,
                },
            }
        }

        if !internal_msg_batch.is_empty() {
            let msg_batch: Vec<crate::Message> = internal_msg_batch
                .iter()
                .map(|i| i.message.clone())
                .collect();
            // println!("writing batch of {}", msg_batch.len());
            match o.write_batch(msg_batch).await {
                Ok(_) => {
                    now = time::SystemTime::now();
                    for msg in internal_msg_batch {
                        state
                            .send(InternalMessageState {
                                message_id: msg.message_id,
                                status: MessageStatus::Output,
                            })
                            .await
                            .unwrap();
                    }
                }
                Err(e) => match e {
                    Error::ConditionalCheckfailed => {
                        debug!("conditional check failed for output");
                    }
                    _ => {
                        for msg in internal_msg_batch {
                            state
                                .send(InternalMessageState {
                                    message_id: msg.message_id,
                                    status: MessageStatus::OutputError(format!("{}", e)),
                                })
                                .await
                                .unwrap();
                        }
                    }
                },
            };
        } else {
            now = time::SystemTime::now();
        }
    }

    println!("exiting output batch");
    o.close().await?;
    state
        .send(InternalMessageState {
            message_id: "end of the line".into(),
            status: MessageStatus::Shutdown,
        })
        .await
        .unwrap();
    Ok(())
}
