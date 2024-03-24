use crossbeam_channel::TryRecvError;
use futures::select;
use futures::join;
use futures::stream::FuturesOrdered;
use futures::stream::StreamExt;
use futures::future::FutureExt;
use tokio::task::yield_now;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::Once;

use super::Error;
use super::Message;
use crate::config::{ParsedConfig, Config, ParsedRegisteredItem, ParsedPipeline};
use crate::config::ExecutionType;
use super::Callback;

use crate::modules::inputs;
use crate::modules::outputs;
use crate::modules::processors;

use crossbeam_channel::unbounded;
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
        
        let conf: Config = serde_yaml::from_str(config)?;
        let parsed_conf = conf.validate()?;

        Ok(Environment{
            config: parsed_conf,
        })
    }

    pub async fn run(self: &Self) -> Result<(), Error> {
        let (input_tx, input_rx) = unbounded();
        let (processor_tx, processor_rx) = unbounded();
        let input = self.input(&self.config.input, input_tx);
        let processors = self.pipeline(&self.config.pipeline, input_rx, processor_tx);
        let output = self.output(&self.config.output, processor_rx);

        println!("waiting on futures");

        let (input_result, proc_result, output_result) = join!(input, processors, output);
        input_result?;
        proc_result?;
        output_result?;

        Ok(())
    }


    async fn input(&self, input: &ParsedRegisteredItem, output: Sender<InternalMessage>) -> Result<(), Error> {
        println!("started input");
        let i = match &input.execution_type {
            ExecutionType::Input(i) => i,
            _ => return Err(Error::Validation("invalid execution type".into())),
        };

        i.connect()?;

        let mut future = i.read().fuse();

        loop {
            select! {
                m = future => {
                    match m {
                        Ok((msg, closer)) => {
                            let internal_msg = InternalMessage{
                                original: msg.clone(),
                                message: msg,
                                closure: closer,
                                active_count: Arc::new(Mutex::new(1)),
                            };
                            // println!("received input message");
                            output.send(internal_msg).map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                            // println!("input message sent");
                            future = i.read().fuse();
                        },
                        Err(e) => {
                            i.close()?;
                            match e {
                                Error::EndOfInput => {
                                    println!("exiting input");
                                    return Ok(())
                                },
                                _ => return Err(Error::ExecutionError(format!("Received error from read: {}", e))),
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
        // eprintln!("pipeline entered");
        let mut channels: HashMap<usize, InternalChannel> = HashMap::new();
        for n in 0..pipeline.processors.len() {
            let (tx, rx) = unbounded();
            channels.insert(n, InternalChannel{
                tx,
                rx,
            });
        };

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

        // println!("futures ready");
        let forwarder = self.channel_forward(rx, output).fuse();
        let futures = FuturesOrdered::from_iter(pipelines).fuse();
        let future_to_await = futures.collect::<Vec<Result<(), Error>>>();
        // println!("waiting on pipelines");
        futures::pin_mut!(forwarder, future_to_await);

        // drop channels so processors will close once input does
        drop(channels);

        // let results = futures.collect::<Vec<Result<(), Error>>>();
        // let (fwd_output, vec_output) = join!(forwarder, futures.collect::<Vec<Result<(), Error>>>());
        'outer: loop {
            // println!("Checking status");
            select! {
                fwd_output = forwarder => fwd_output?,
                results = future_to_await => {
                    println!("processor finished");
                    for r in results {
                        r?;
                    };
                },
                complete => {
                    println!("breaking outer");
                    break 'outer
                },
                default => yield_now().await,
            };
            yield_now().await
        };
        println!("pipelines done");

        Ok(())


    }

    async fn run_processor(&self, input: Receiver<InternalMessage>, output: Sender<InternalMessage>, processor: &ParsedRegisteredItem) -> Result<(), Error> {
        println!("Started processor");
        let p = match &processor.execution_type {
            ExecutionType::Processor(p) => p,
            _ => return Err(Error::Validation("invalid execution type".into())),
        };

        loop {
            match input.try_recv() {
                Ok(msg) => {
                    // println!("Received processing message");
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
        
                                output.send(new_msg).map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                                
                            };
                        },
                        Err(e) => {
                            println!("Processing error {}", e);
                            match e {
                                Error::ConditionalCheckfailed => {
                                    output.send(msg).map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;                                    
                                },
                                _ => return Err(e),
                            }
                        }
                    }
                    
                },
                Err(e) => {
                    match e {
                        TryRecvError::Disconnected => {
                            p.close()?;
                            println!("exiting processor");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                    // return Err(Error::UnableToSendToChannel(format!("{}", e)))
                },
            };
            yield_now().await;
        };
    }

    async fn output(self: &Self, output: &ParsedRegisteredItem, input: Receiver<InternalMessage>) -> Result<(), Error> {
        println!("started output");
        let o = match &output.execution_type {
            ExecutionType::Output(o) => o,
            _ => return Err(Error::Validation("invalid execution type".into())),
        };

        o.connect()?;

        loop {
            match input.try_recv() {
                Ok(msg) => {
                    // println!("writing message");
                    // o.write(msg.message.clone()).await.map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

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
                            println!("Processing error {}", e);
                            match e {
                                Error::ConditionalCheckfailed => {
                                    println!("Failed conditional check on output");
                                },
                                _ => {
                                    println!("Found another error {}", e);
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
                            println!("exiting output");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                    // return Err(Error::UnableToSendToChannel(format!("{}", e)))
                },
            };
            yield_now().await;
        }
    }

    async fn channel_forward(&self, input: Receiver<InternalMessage>, output: Sender<InternalMessage>) -> Result<(), Error> {
        loop {
            match input.try_recv() {
                Ok(msg) => output.send(msg).map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?,
                Err(e) => {
                    match e {
                        TryRecvError::Disconnected => {
                            println!("exiting forwarder");
                            return Ok(())
                        },
                        TryRecvError::Empty => {}
                    }
                    // return Err(Error::UnableToSendToChannel(format!("{}", e)))
                },
            };
            yield_now().await;
        };
    }
}