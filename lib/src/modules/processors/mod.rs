use crate::Error;
pub mod lines;
pub mod noop;
#[cfg(feature = "python")]
pub mod python;
pub mod switch;

use crate::config::{ExecutionType, ParsedRegisteredItem};
use crate::runtime::{InternalMessage, InternalMessageState, MessageStatus};
use flume::{Receiver, Sender, TryRecvError};
use tokio::task::yield_now;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, trace};

pub(crate) fn register_plugins() -> Result<(), Error> {
    lines::register_lines()?;
    noop::register_noop()?;
    #[cfg(feature = "python")]
    python::register_python()?;
    switch::register_switch()?;
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
    let mut p = match (processor.creator)(&processor.config)? {
        ExecutionType::Processor(p) => p,
        _ => {
            error!("invalid execution type for processor");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    loop {
        match input.try_recv() {
            Ok(msg) => {
                trace!("received processing message");
                match p.process(msg.message.clone()).await {
                    Ok(m) => {
                        if m.len() > 1 {
                            for _ in 0..(m.len() - 1) {
                                state_tx
                                    .send_async(InternalMessageState {
                                        message_id: msg.message_id.clone(),
                                        status: MessageStatus::New,
                                    })
                                    .await
                                    .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                            }
                        }

                        for m in m.iter() {
                            let mut new_msg = msg.clone();
                            new_msg.message = m.clone();
                            trace!("message processed");
                            output.send_async(new_msg)
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        }
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for processor");

                            state_tx
                                .send_async(InternalMessageState {
                                    message_id: msg.message_id,
                                    status: MessageStatus::ProcessError(format!("{}", e)),
                                })
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        }
                        _ => {
                            error!(error = format!("{}", e), "read error from processor");
                            return Err(e);
                        }
                    },
                }
            }
            Err(e) => match e {
                TryRecvError::Disconnected => {
                    p.close().await?;
                    debug!("processor closed");
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
        yield_now().await;
    }
}
