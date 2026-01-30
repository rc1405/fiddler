use crate::Error;
pub mod compression;
pub mod decode;
pub mod exception;
pub mod fiddlerscript;
pub mod lines;
pub mod noop;
#[cfg(feature = "python")]
pub mod python;
pub mod switch;

use crate::config::{ExecutionType, ParsedRegisteredItem};
use crate::runtime::{InternalMessage, InternalMessageState, MessageStatus};
use flume::{Receiver, Sender};
use tracing::{debug, error, trace};

pub(crate) fn register_plugins() -> Result<(), Error> {
    lines::register_lines()?;
    noop::register_noop()?;
    #[cfg(feature = "python")]
    python::register_python()?;
    switch::register_switch()?;
    compression::register_compress()?;
    decode::register_decode()?;
    exception::register_try()?;
    fiddlerscript::register_fiddlerscript()?;
    Ok(())
}

pub(crate) async fn run_processor(
    processor: ParsedRegisteredItem,
    output: Sender<InternalMessage>,
    input: Receiver<InternalMessage>,
    state_tx: Sender<InternalMessageState>,
) -> Result<(), Error> {
    trace!("Started processor");
    // let proc = (processor.creator)(&processor.config)?;
    let mut p = match (processor.creator)(processor.config.clone()).await? {
        ExecutionType::Processor(p) => p,
        _ => {
            error!("invalid execution type for processor");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    loop {
        match input.recv_async().await {
            Ok(msg) => {
                trace!("received processing message");
                let stream_id = msg.message.stream_id.clone();
                let message_id = msg.message_id.clone();
                let status = msg.status.clone();

                match p.process(msg.message).await {
                    Ok(m) => {
                        let msg_count = m.len();
                        if msg_count > 1 {
                            for _ in 0..(msg_count - 1) {
                                state_tx
                                    .send_async(InternalMessageState {
                                        message_id: message_id.clone(),
                                        status: MessageStatus::New,
                                        stream_id: stream_id.clone(),
                                        ..Default::default()
                                    })
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                            }
                        }

                        for message in m.into_iter() {
                            let new_msg = InternalMessage {
                                message_id: message_id.clone(),
                                status: status.clone(),
                                message: crate::Message {
                                    stream_id: stream_id.clone(),
                                    ..message
                                },
                            };

                            trace!("message processed");
                            output
                                .send_async(new_msg)
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                        }
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for processor");

                            state_tx
                                .send_async(InternalMessageState {
                                    message_id,
                                    status: MessageStatus::ProcessError(format!("{e}")),
                                    stream_id,
                                    ..Default::default()
                                })
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                        }
                        _ => {
                            error!(error = format!("{e}"), "read error from processor");
                            return Err(e);
                        }
                    },
                }
            }
            Err(_) => {
                // Channel disconnected - clean shutdown
                p.close().await?;
                debug!("processor closed");
                return Ok(());
            }
        }
    }
}
