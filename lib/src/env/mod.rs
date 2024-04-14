use crossbeam_channel::TryRecvError;
use serde_yaml::Value;
use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio::task::JoinSet;
use tracing::{debug, error, info, trace};

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::sync::Once;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

use super::CallbackChan;
use super::Error;
use super::Message;
use crate::config::parse_configuration_item;
use crate::config::ExecutionType;
use crate::config::{Config, ItemType, ParsedConfig, ParsedPipeline, ParsedRegisteredItem};

use crate::modules::inputs;
use crate::modules::outputs;
use crate::modules::processors;
use crate::Status;

use crossbeam_channel::bounded;
use crossbeam_channel::{Receiver, Sender};

static REGISTER: Once = Once::new();

/// Represents a single data pipeline configuration environment to run
pub struct Environment {
    config: ParsedConfig,
}

#[derive(Clone)]
struct InternalMessage {
    message: Message,
    closure: Arc<CallbackChan>,
    active_count: Arc<Mutex<RefCell<usize>>>,
    errors: Arc<Mutex<RefCell<Vec<String>>>>,
}

#[derive(Clone)]
struct InternalChannel {
    tx: Sender<InternalMessage>,
    rx: Receiver<InternalMessage>,
}

impl Environment {
    /// The function takes the raw configuration of the data pipeline and registers built-in
    /// plugins, validates the configuration and returns the Environment to run.
    /// ```
    /// use fiddler::Environment;
    ///
    /// let conf_str = r#"input:
    ///  stdin: {}
    ///pipeline:
    ///  processors:
    ///    - label: my_cool_mapping
    ///      noop: {}
    ///output:
    ///  stdout: {}"#;
    /// let env = Environment::from_config(conf_str).unwrap();
    /// ```
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
        Ok(Environment {
            config: parsed_conf,
        })
    }

    /// The function sets the data pipeline with a label.
    /// ```
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    /// env.set_label(Some("MyPipeline".into())).unwrap();
    /// ```
    /// or to remove a given label:
    /// ```
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #  stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    /// env.set_label(None).unwrap();
    /// ```
    pub fn set_label(&mut self, label: Option<String>) -> Result<(), Error> {
        self.config.label = label;
        Ok(())
    }

    /// The function returns the currect label assigned to the pipeline
    /// ```
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    /// # env.set_label(Some("MyPipeline".into())).unwrap();
    /// assert_eq!(env.get_label().unwrap(), "MyPipeline".to_string());
    /// ```
    pub fn get_label(&self) -> Option<String> {
        self.config.label.clone()
    }

    /// The function replaces the existing input configuration with the provided input.
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use fiddler::modules::inputs;
    /// # use std::collections::HashMap;
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    ///
    /// use serde_yaml::Value;
    /// let conf_str = r#"file:
    ///    filename: tests/data/input.txt
    ///    codec: ToEnd"#;
    /// let parsed_input: HashMap<String, Value> = serde_yaml::from_str(conf_str).unwrap();
    ///
    /// env.set_input(&parsed_input).unwrap()
    /// ```
    pub fn set_input(&mut self, input: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Input, input)?;
        self.config.input = parsed_item;
        Ok(())
    }

    /// The function replaces the existing output configuration with the provided output.
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use fiddler::modules::inputs;
    /// # use std::collections::HashMap;
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    ///
    /// use serde_yaml::Value;
    /// let conf_str = r#"stdout: {}"#;
    /// let parsed_output: HashMap<String, Value> = serde_yaml::from_str(conf_str).unwrap();
    ///
    /// env.set_output(&parsed_output).unwrap()
    /// ```
    pub fn set_output(&mut self, output: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Output, output)?;
        self.config.output = parsed_item;
        Ok(())
    }

    /// The function runs the existing data pipeline until receiving an Error::EndOfInput
    /// ```no_run
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use fiddler::modules::inputs;
    /// # use std::collections::HashMap;
    /// # use fiddler::Environment;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Environment::from_config(conf_str).unwrap();
    /// # tokio_test::block_on(async {
    /// env.run().await.unwrap();
    /// # })
    /// ```
    pub async fn run(&self) -> Result<(), Error> {
        let (processor_tx, processor_rx) = bounded(1);
        let input = input(
            self.config.input.clone(),
            processor_tx,
            self.config.pipeline.clone(),
        );

        let mut handles = JoinSet::new();

        info!("pipeline started");
        handles.spawn(async move { input.await });

        let output = output(self.config.output.clone(), processor_rx.clone());
        handles.spawn(async move { output.await });

        while let Some(res) = handles.join_next().await {
            res.map_err(|e| Error::ProcessingError(format!("{}", e)))??;
        }

        info!("pipeline finished");
        Ok(())
    }
}

async fn yield_sender(chan: &Sender<InternalMessage>, msg: InternalMessage) -> Result<(), Error> {
    while chan.is_full() {
        yield_now().await
    }

    chan.send(msg)
        .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))
}

async fn input(
    input: ParsedRegisteredItem,
    output: Sender<InternalMessage>,
    parsed_pipeline: ParsedPipeline,
) -> Result<(), Error> {
    trace!("started input");
    let i = match &input.execution_type {
        ExecutionType::Input(i) => i,
        _ => {
            error!("invalid execution type for input");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    let task_count = Arc::new(());
    let mut max_tasks = parsed_pipeline.max_in_flight.clone();
    if max_tasks == 0 {
        max_tasks = num_cpus::get();
    };
    max_tasks += 1;
    debug!("max_in_flight count: {}", max_tasks);

    i.connect()?;
    debug!("input connected");

    loop {
        match i.read().await {
            Ok((msg, closer)) => {
                trace!("message received from input");
                let internal_msg = InternalMessage {
                    message: msg,
                    closure: Arc::new(closer),
                    active_count: Arc::new(Mutex::new(RefCell::new(1))),
                    errors: Arc::new(Mutex::new(RefCell::new(Vec::new()))),
                };

                while Arc::strong_count(&task_count) >= max_tasks {
                    trace!("waiting for open thread");
                    sleep(Duration::from_millis(50)).await;
                }

                let p = pipeline(
                    parsed_pipeline.clone(),
                    internal_msg,
                    output.clone(),
                    task_count.clone(),
                );

                tokio::spawn(async move {
                    let r = p.await;
                    r
                });
            }
            Err(e) => {
                match e {
                    Error::EndOfInput => {
                        i.close()?;
                        debug!("input closed");
                        info!("shutting down input: end of input received");
                        return Ok(());
                    }
                    Error::NoInputToReturn => {
                        sleep(Duration::from_millis(1500)).await;
                        // std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                    _ => {
                        i.close()?;
                        debug!("input closed");
                        error!(error = format!("{}", e), "read error from input");
                        return Err(Error::ExecutionError(format!(
                            "Received error from read: {}",
                            e
                        )));
                    }
                }
            }
        }
    }
}

async fn pipeline(
    pipeline: ParsedPipeline,
    input: InternalMessage,
    output: Sender<InternalMessage>,
    _task_count: Arc<()>,
) -> Result<(), Error> {
    trace!("starting pipeline");
    let (tx, mut rx) = bounded(1);
    yield_sender(&tx, input).await?;
    drop(tx);

    let mut channels: HashMap<usize, InternalChannel> = HashMap::new();
    for n in 0..pipeline.processors.len() {
        let (tx, rx) = bounded(pipeline.processors.len());
        channels.insert(n, InternalChannel { tx, rx });
    }

    debug!(num_processors = channels.len(), "starting processors");

    let mut pipelines = JoinSet::new();

    for i in 0..pipeline.processors.len() {
        let ic = channels.get(&i).unwrap().clone();
        let p = pipeline.processors[i].clone();

        pipelines.spawn(async move { run_processor(rx, ic.tx, p).await });

        rx = ic.rx;
    }

    pipelines.spawn(async move { channel_forward(rx, output).await });

    // drop channels so processors will close once finished
    drop(channels);

    while let Some(res) = pipelines.join_next().await {
        res.map_err(|e| Error::ProcessingError(format!("{}", e)))??;
    }

    debug!("shutting down pipeline");

    Ok(())
}

async fn run_processor(
    input: Receiver<InternalMessage>,
    output: Sender<InternalMessage>,
    processor: ParsedRegisteredItem,
) -> Result<(), Error> {
    trace!("Started processor");
    let p = match &processor.execution_type {
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
                        for (i, m) in m.iter().enumerate() {
                            let mut new_msg = msg.clone();
                            new_msg.message = m.clone();
                            if i > 0 {
                                match new_msg.active_count.lock() {
                                    Ok(l) => {
                                        let mut lock = l.borrow_mut();
                                        *lock += 1;
                                    }
                                    Err(_) => return Err(Error::UnableToSecureLock),
                                };
                            };

                            yield_sender(&output, new_msg).await?;
                        }
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for processor");

                            yield_sender(&output, msg).await?;
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
                    p.close()?;
                    debug!("processor closed");
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
        yield_now().await;
    }
}

async fn output(
    output: ParsedRegisteredItem,
    input: Receiver<InternalMessage>,
) -> Result<(), Error> {
    trace!("started output");
    let o = match &output.execution_type {
        ExecutionType::Output(o) => o,
        _ => {
            error!("invalid execution type for output");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    o.connect()?;
    debug!("output connected");

    loop {
        match input.try_recv() {
            Ok(mut msg) => {
                trace!("received output message");
                match o.write(msg.message.clone()).await {
                    Ok(_) => {
                        let active_count = decrease_active_count(&mut msg).await?;
                        if active_count == 0 {
                            let errs = get_errors(&msg).await;
                            let status = match errs.len() {
                                0 => Status::Processed,
                                _ => Status::Errored(errs),
                            };

                            send_status(&mut msg, status).await?;
                        };
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for output");
                        }
                        _ => {
                            add_error(&mut msg, format!("{}", e)).await?;
                            let active_count = decrease_active_count(&mut msg).await?;
                            if active_count == 0 {
                                let errs = get_errors(&msg).await;
                                send_status(&mut msg, Status::Errored(errs)).await?;
                            };
                            error!(error = format!("{}", e), "write error from output");
                        }
                    },
                }
            }
            Err(e) => match e {
                TryRecvError::Disconnected => {
                    o.close()?;
                    debug!("output closed");
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
    }
}

async fn channel_forward(
    input: Receiver<InternalMessage>,
    output: Sender<InternalMessage>,
) -> Result<(), Error> {
    loop {
        match input.try_recv() {
            Ok(msg) => {
                yield_sender(&output, msg).await?;
            }
            Err(e) => match e {
                TryRecvError::Disconnected => {
                    trace!("shutting down output forwarder");
                    return Ok(());
                }
                TryRecvError::Empty => {}
            },
        };
        yield_now().await;
    }
}

async fn get_errors(message: &InternalMessage) -> Vec<String> {
    match message.errors.lock() {
        Ok(l) => {
            let err = l.borrow_mut();
            let errs: Vec<String> = err.iter().map(|e| format!("{}", e)).collect();
            return errs;
        }
        Err(_) => return Vec::new(),
    };
}

async fn decrease_active_count(message: &mut InternalMessage) -> Result<usize, Error> {
    match message.active_count.lock() {
        Ok(l) => {
            let mut lock = l.borrow_mut();
            *lock -= 1;
            trace!("message has {} copies to process", lock);
            return Ok(lock.clone());
        }
        Err(_) => return Err(Error::UnableToSecureLock),
    };
}

async fn add_error(message: &mut InternalMessage, error: String) -> Result<(), Error> {
    match message.errors.lock() {
        Ok(l) => {
            let mut err = l.borrow_mut();
            err.push(error);
            Ok(())
        }
        Err(_) => Err(Error::UnableToSecureLock),
    }
}

async fn send_status(message: &mut InternalMessage, status: Status) -> Result<(), Error> {
    let (s, _) = oneshot::channel();
    let chan = mem::replace(&mut message.closure, Arc::new(s));
    let c = Arc::into_inner(chan).unwrap();
    let _ = c.send(status);
    Ok(())
}
