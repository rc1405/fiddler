use crossbeam_channel::TryRecvError;
use futures::select;
use futures::join;
use futures::stream::FuturesOrdered;
use futures::stream::StreamExt;
use futures::future::FutureExt;
use tokio::task::yield_now;
use tracing::{debug, error, info, trace};
use serde_yaml::Value;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::Once;

use super::Error;
use super::Message;
use crate::config::parse_configuration_item;
use crate::config::{ParsedConfig, Config, ParsedRegisteredItem, ParsedPipeline, ItemType};
use crate::config::ExecutionType;
use super::Callback;

use crate::modules::inputs;
use crate::modules::outputs;
use crate::modules::processors;

use crossbeam_channel::bounded;
use crossbeam_channel::{Sender, Receiver};

static REGISTER: Once = Once::new();

pub struct Environment {
    config: ParsedConfig,
}

#[derive(Clone)]
pub struct InternalMessage {
    original: Message,
    message: Message,
    closure: Callback,
    active_count: Arc<Mutex<usize>>,
}

#[derive(Clone)]
struct InternalChannel {
    tx: Sender<InternalMessage>,
    rx: Receiver<InternalMessage>,
}

impl Environment {
    pub fn from_config(config: &str) -> Result<Self, Error> {
        REGISTER.call_once(|| {
            inputs::register_plugins().unwrap();
            outputs::register_plugins().unwrap();
            processors::register_plugins().unwrap();
        });
        trace!("plugins registered");
        
        let conf: Config = serde_yaml::from_str(config)?;
        let parsed_conf = conf.validate()?;

        debug!("environment is ready");
        Ok(Environment{
            config: parsed_conf,
        })
    }

    pub fn set_label(&mut self, label: Option<String>) -> Result<(), Error> {
        self.config.label = label;
        Ok(())
    }

    pub fn get_label(&self) -> Option<String> {
        self.config.label.clone()
    }

    pub fn set_input(&mut self, input: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Input, input)?;
        self.config.input = parsed_item;
        Ok(())
    }

    pub fn set_output(&mut self, output: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Output, output)?;
        self.config.output = parsed_item;
        Ok(())
    }

    pub async fn run(self: &Self) -> Result<(), Error> {
        let (input_tx, input_rx) = bounded(self.config.pipeline.max_in_flight.clone());
        let (processor_tx, processor_rx) = bounded(1);
        let input = self.input(&self.config.input, input_tx);
        let processors = self.pipeline(&self.config.pipeline, input_rx, processor_tx);
        let output = self.output(&self.config.output, processor_rx);

        info!("pipeline started");

        let (input_result, proc_result, output_result) = join!(input, processors, output);
        input_result?;
        proc_result?;
        output_result?;

        info!("pipeline finished");
        Ok(())
    }

    async fn yield_sender(&self, chan: &Sender<InternalMessage>, msg: InternalMessage) -> Result<(), Error> {
        while chan.is_full() {
            yield_now().await
        };

        chan.send(msg).map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))
    }

    async fn input(&self, input: &ParsedRegisteredItem, output: Sender<InternalMessage>) -> Result<(), Error> {
        trace!("started input");
        let i = match &input.execution_type {
            ExecutionType::Input(i) => i,
            _ => {
                error!("invalid execution type for input");
                return Err(Error::Validation("invalid execution type".into()))
            },
        };

        i.connect()?;
        debug!("input connected");

        let mut future = i.read().fuse();

        loop {
            select! {
                m = future => {
                    match m {
                        Ok((msg, closer)) => {
                            trace!("message received from input");
                            let internal_msg = InternalMessage{
                                original: msg.clone(),
                                message: msg,
                                closure: closer,
                                active_count: Arc::new(Mutex::new(1)),
                            };

                            self.yield_sender(&output, internal_msg).await?;
                            future = i.read().fuse();
                        },
                        Err(e) => {
                            i.close()?;
                            debug!("input closed");
                            match e {
                                Error::EndOfInput => {
                                    info!("shutting down input: end of input received");
                                    return Ok(())
                                },
                                _ => {
                                    error!(error = format!("{}", e), "read error from input");
                                    return Err(Error::ExecutionError(format!("Received error from read: {}", e)))
                                },
                            }                    
                        },
                    }
                },
                default => yield_now().await
            };
            yield_now().await
        };

    }

    async fn pipeline(&self, pipeline: &ParsedPipeline, input: Receiver<InternalMessage>, output: Sender<InternalMessage>) -> Result<(), Error> {
        trace!("starting pipeline");
        let mut channels: HashMap<usize, InternalChannel> = HashMap::new();
        for n in 0..pipeline.processors.len() {
            let (tx, rx) = bounded(pipeline.processors.len());
            channels.insert(n, InternalChannel{
                tx,
                rx,
            });
        };

        debug!(num_processors = channels.len(), "starting processors");

        let mut pipelines = Vec::new();
        let mut rx: Receiver<InternalMessage> = input;
        for (i, v) in pipeline.processors.iter().enumerate() {
            // let index = i + 1;
            let ic = channels.get(&i).unwrap().clone();

            pipelines.push(self.run_processor(
                rx,
                ic.tx,
                v,
            ));

            rx = ic.rx;
        };

        
        let forwarder = self.channel_forward(rx, output).fuse();
        let futures = FuturesOrdered::from_iter(pipelines).fuse();
        let future_to_await = futures.collect::<Vec<Result<(), Error>>>();
        futures::pin_mut!(forwarder, future_to_await);

        // drop channels so processors will close once input does
        drop(channels);

        trace!("waiting for processors to finish");
        'outer: loop {
            select! {
                fwd_output = forwarder => fwd_output?,
                results = future_to_await => {
                    debug!("processor finished");
                    for r in results {
                        match r {
                            Ok(_) => {},
                            Err(e) => {
                                error!(error = format!("{}", e), "processing error");
                                return Err(e)
                            }
                        }
                    };
                },
                complete => {
                    debug!("processors completed");
                    break 'outer
                },
                default => yield_now().await,
            };
            yield_now().await
        };
        debug!("shutting down pipeline");

        Ok(())


    }

    async fn run_processor(&self, input: Receiver<InternalMessage>, output: Sender<InternalMessage>, processor: &ParsedRegisteredItem) -> Result<(), Error> {
        trace!("Started processor");
        let p = match &processor.execution_type {
            ExecutionType::Processor(p) => p,
            _ => {
                error!("invalid execution type for processor");
                return Err(Error::Validation("invalid execution type".into()))
            },
        };

        loop {
            match input.try_recv() {
                Ok(msg) => {
                    trace!("received processing message");
                    match p.process(msg.message.clone()).await {
                        Ok(m) => {
                            for i in m {
                                let mut new_msg = msg.clone();
                                new_msg.message = i;
                                match new_msg.active_count.lock() {
                                    Ok(mut lock) => {
                                        *lock += 1;
                                    },
                                    Err(_) => return Err(Error::UnableToSecureLock),
                                };

                                self.yield_sender(&output, new_msg).await?;
                                
                            };
                        },
                        Err(e) => {
                            match e {
                                Error::ConditionalCheckfailed => {
                                    debug!("conditional check failed for processor");

                                    self.yield_sender(&output, msg).await?;                                  
                                },
                                _ => {
                                    error!(error = format!("{}", e), "read error from processor");
                                    return Err(e)
                                },
                            }
                        }
                    }
                    
                },
                Err(e) => {
                    match e {
                        TryRecvError::Disconnected => {
                            p.close()?;
                            debug!("processor closed");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                },
            };
            yield_now().await;
        };
    }

    async fn output(self: &Self, output: &ParsedRegisteredItem, input: Receiver<InternalMessage>) -> Result<(), Error> {
        trace!("started output");
        let o = match &output.execution_type {
            ExecutionType::Output(o) => o,
            _ => {
                error!("invalid execution type for output");
                return Err(Error::Validation("invalid execution type".into()))
            },
        };

        o.connect()?;
        debug!("output connected");

        loop {
            match input.try_recv() {
                Ok(msg) => {
                    trace!("received output message");
                    match o.write(msg.message.clone()).await {
                        Ok(_) => {
                            match msg.active_count.lock() {
                                Ok(mut lock) => {
                                    *lock -= 1;
                                    if *lock == 0 {
                                        (msg.closure)(msg.original)?
                                    }
                                },
                                Err(_) => return Err(Error::UnableToSecureLock),
                            };
                        },
                        Err(e) => {
                            match e {
                                Error::ConditionalCheckfailed => {
                                    debug!("conditional check failed for output");
                                },
                                _ => {
                                    error!(error = format!("{}", e), "write error from output");
                                    return Err(e)
                                },
                            }
                        }
                    }                    
                },
                Err(e) => {
                    match e {
                        TryRecvError::Disconnected => {
                            o.close()?;
                            debug!("output closed");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                },
            };
            yield_now().await;
        }
    }

    async fn channel_forward(&self, input: Receiver<InternalMessage>, output: Sender<InternalMessage>) -> Result<(), Error> {
        loop {
            match input.try_recv() {
                Ok(msg) => {
                    self.yield_sender(&output, msg).await?;
                },
                Err(e) => {
                    match e {
                        TryRecvError::Disconnected => {
                            trace!("shutting down output forwarder");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                },
            };
            yield_now().await;
        };
    }
}