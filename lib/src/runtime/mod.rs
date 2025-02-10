use async_channel::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use std::fmt;
use std::mem;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tracing::{debug, error, info, trace};

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Once;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::CallbackChan;
use super::Error;
use super::Message;
use crate::config::parse_configuration_item;
use crate::config::ExecutionType;
use crate::config::{Config, ItemType, ParsedConfig, ParsedRegisteredItem, RegisteredItem};

use crate::modules::inputs;
use crate::modules::outputs;
use crate::modules::processors;
use crate::Status;

static REGISTER: Once = Once::new();

/// Represents a single data pipeline configuration Runtime to run
pub struct Runtime {
    config: ParsedConfig,
    state_tx: Sender<InternalMessageState>,
    state_rx: Receiver<InternalMessageState>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) enum MessageStatus {
    #[default]
    New,
    Processed,
    ProcessError(String),
    Output,
    OutputError(String),
    Shutdown,
}

impl fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            MessageStatus::New => write!(f, "New"),
            MessageStatus::Processed => write!(f, "Processed"),
            MessageStatus::ProcessError(_) => write!(f, "ProcessError"),
            MessageStatus::Output => write!(f, "Output"),
            MessageStatus::OutputError(_) => write!(f, "OutputError"),
            MessageStatus::Shutdown => write!(f, "Shutdown"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) enum ProcessingState {
    #[default]
    InputComplete,
    ProcessorSubscribe((usize, String)),
    ProcessorComplete(usize),
    PipelineComplete,
    OutputSubscribe(String),
    OutputComplete,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InternalMessageState {
    pub message_id: String,
    pub status: MessageStatus,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InternalMessage {
    pub message: Message,
    pub message_id: String,
    pub status: MessageStatus,
}

#[derive(Clone)]
struct MessageHandle {
    message_id: String,
    closure: Arc<CallbackChan>,
}

impl Runtime {
    /// The function takes the raw configuration of the data pipeline and registers built-in
    /// plugins, validates the configuration and returns the Runtime to run.
    /// ```
    /// use fiddler::Runtime;
    ///
    /// let conf_str = r#"input:
    ///  stdin: {}
    ///pipeline:
    ///  processors:
    ///    - label: my_cool_mapping
    ///      noop: {}
    ///output:
    ///  stdout: {}"#;
    /// let env = Runtime::from_config(conf_str).unwrap();
    /// ```
    pub fn from_config(config: &str) -> Result<Self, Error> {
        REGISTER.call_once(|| {
            inputs::register_plugins().unwrap();
            outputs::register_plugins().unwrap();
            processors::register_plugins().unwrap();
        });
        trace!("plugins registered");

        let conf: Config = Config::from_str(config)?;
        let parsed_conf = conf.validate()?;

        let (state_tx, state_rx) = bounded(100);

        debug!("Runtime is ready");
        Ok(Runtime {
            config: parsed_conf,
            state_rx,
            state_tx,
        })
    }

    /// The function sets the data pipeline with a label.
    /// ```
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
    /// env.set_label(Some("MyPipeline".into())).unwrap();
    /// ```
    /// or to remove a given label:
    /// ```
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #  stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
    /// env.set_label(None).unwrap();
    /// ```
    pub fn set_label(&mut self, label: Option<String>) -> Result<(), Error> {
        self.config.label = label;
        Ok(())
    }

    /// The function returns the currect label assigned to the pipeline
    /// ```
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
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
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
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
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
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
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
    /// # tokio_test::block_on(async {
    /// env.run().await.unwrap();
    /// # })
    /// ```
    pub async fn run(&self) -> Result<(), Error> {
        let mut handles = JoinSet::new();

        let (msg_tx, msg_rx) = bounded(10000);
        let msg_state = message_handler(msg_rx, self.state_rx.clone());
        handles.spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("state")
                .worker_threads(1)
                // .on_thread_start(move || set_current_thread_priority_low())
                .build()
                .expect("Creating tokio runtime");
            runtime.block_on(msg_state)
        });

        println!("Starting output in run");
        let output = self
            .output(self.config.output.clone(), &mut handles)
            .await?;
        println!("Started output");

        let processors = self.pipeline(output, &mut handles).await?;

        let input = input(self.config.input.clone(), processors, msg_tx);

        handles.spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("input")
                .worker_threads(1)
                // .on_thread_start(move || set_current_thread_priority_low())
                .build()
                .expect("Creating tokio runtime");
            runtime.block_on(input)
        });

        info!("pipeline started");

        while let Some(res) = handles.join_next().await {
            res.map_err(|e| Error::ProcessingError(format!("{}", e)))??;
        }

        info!("pipeline finished");
        Ok(())
    }

    async fn pipeline(
        &self,
        input: Sender<InternalMessage>,
        handles: &mut JoinSet<Result<(), Error>>,
    ) -> Result<Sender<InternalMessage>, Error> {
        trace!("starting pipeline");

        let mut processors = self.config.processors.clone();
        processors.reverse();

        let mut next_tx = input;

        for i in 0..processors.len() {
            let p = processors[i].clone();

            let (tx, rx) = bounded(100);

            for n in 0..self.config.num_threads {
                // tokio::spawn(processors::run_processor(p.clone(), next_tx.clone(), rx.clone(), self.state_tx.clone()));
                let proc = processors::run_processor(
                    p.clone(),
                    next_tx.clone(),
                    rx.clone(),
                    self.state_tx.clone(),
                );
                handles.spawn_blocking(move || {
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .thread_name(format!("processor{}-{}", i, n))
                        .worker_threads(1)
                        // .on_thread_start(move || set_current_thread_priority_low())
                        .build()
                        .expect("Creating tokio runtime");
                    runtime.block_on(proc)
                });
            }

            next_tx = tx;
        }

        Ok(next_tx)
    }

    async fn output(
        &self,
        output: ParsedRegisteredItem,
        handles: &mut JoinSet<Result<(), Error>>,
    ) -> Result<Sender<InternalMessage>, Error> {
        trace!("started output");

        let (tx, rx) = bounded(100);

        for i in 0..self.config.num_threads {
            println!("Starting output thread {}", i);
            let item = (output.creator)(&output.config)?;
            match item {
                ExecutionType::Output(o) => {
                    // handles.spawn(outputs::run_output(rx.clone(), self.state_tx.clone(), o));
                    let state_tx = self.state_tx.clone();
                    let new_rx = rx.clone();
                    handles.spawn_blocking(move || {
                        let runtime = tokio::runtime::Builder::new_multi_thread()
                            .enable_all()
                            .thread_name(format!("output{}", i))
                            .worker_threads(1)
                            // .on_thread_start(move || set_current_thread_priority_low())
                            .build()
                            .expect("Creating tokio runtime");
                        runtime.block_on(outputs::run_output(new_rx, state_tx, o))
                    });
                }
                ExecutionType::OutputBatch(o) => {
                    // handles.spawn(outputs::run_output_batch(rx.clone(), self.state_tx.clone(), o))
                    let state_tx = self.state_tx.clone();
                    let new_rx = rx.clone();
                    handles.spawn_blocking(move || {
                        let runtime = tokio::runtime::Builder::new_multi_thread()
                            .enable_all()
                            .thread_name(format!("output{}", i))
                            .worker_threads(1)
                            // .on_thread_start(move || set_current_thread_priority_low())
                            .build()
                            .expect("Creating tokio runtime");
                        runtime.block_on(outputs::run_output_batch(new_rx, state_tx, o))
                    });
                }
                _ => {
                    error!("invalid execution type for output");
                    return Err(Error::Validation("invalid execution type".into()));
                }
            };
        }

        Ok(tx)
    }
}

struct State {
    instance_count: i64,
    processed_count: i64,
    processed_error_count: i64,
    output_count: i64,
    output_error_count: i64,
    closure: CallbackChan,
    errors: Vec<String>,
}

async fn message_handler(
    mut new_msg: Receiver<MessageHandle>,
    mut msg_status: Receiver<InternalMessageState>,
) -> Result<(), Error> {
    let mut handles: HashMap<String, State> = HashMap::new();

    loop {
        tokio::select! {
            Ok(msg) = new_msg.recv() => {
                trace!(message_id = msg.message_id, "Received new message");
                let closure = Arc::into_inner(msg.closure).unwrap();
                let i = handles.insert(msg.message_id.clone(), State { instance_count: 1, processed_count: 0, processed_error_count: 0, output_count: 0, output_error_count: 0, closure: closure, errors: Vec::new() });
                if i.is_some() {
                    error!(message_id = msg.message_id, "Received duplicate message");
                    return Err(Error::ExecutionError("Duplicate Message ID Error".into()))
                };
            },
            Ok(msg) = msg_status.recv() => {
                debug!(state = msg.status.to_string(), message_id = msg.message_id, "Received message state");
                match msg.status {
                    MessageStatus::New => {
                        let m = handles.get_mut(&msg.message_id);
                        match m {
                            None => return Err(Error::ExecutionError(format!("Message ID {} does not exist", msg.message_id))),
                            Some(state) => {
                                state.instance_count += 1;
                            },
                        }
                    },
                    MessageStatus::Processed => {
                        let m = handles.get_mut(&msg.message_id);
                        match m {
                            None => return Err(Error::ExecutionError(format!("Message ID {} does not exist", msg.message_id))),
                            Some(state) => {
                                state.processed_count += 1;
                            },
                        }
                    },
                    MessageStatus::ProcessError(s) => {
                        let m = handles.get_mut(&msg.message_id);
                        match m {
                            None => return Err(Error::ExecutionError(format!("Message ID {} does not exist", msg.message_id))),
                            Some(state) => {
                                state.processed_error_count += 1;
                                state.errors.push(s);
                            },
                        }
                    },
                    MessageStatus::Output => {
                        let m = handles.get_mut(&msg.message_id);
                        let mut remove_entry = false;
                        match m {
                            None => return Err(Error::ExecutionError(format!("Message ID {} does not exist", msg.message_id))),
                            Some(state) => {
                                state.output_count += 1;

                                debug!(message_id = msg.message_id, errors = state.processed_error_count, "message fully processed");

                                if state.output_count == state.instance_count {
                                    remove_entry = true;
                                    let (s, _) = oneshot::channel();
                                    let chan = mem::replace(&mut state.closure, s);
                                    let _ = chan.send(Status::Processed);

                                } else if state.instance_count == (state.output_count + state.output_error_count + state.processed_error_count) {
                                    remove_entry = true;
                                    let (s, _) = oneshot::channel();
                                    let chan = mem::replace(&mut state.closure, s);
                                    let e = Vec::new();
                                    let err = mem::replace(&mut state.errors, e);
                                    let _ = chan.send(Status::Errored(err));
                                }
                            },
                        };

                        if remove_entry {
                            trace!(message_id = msg.message_id, "Removing message from state");
                            handles.remove(&msg.message_id);
                        }
                    },
                    MessageStatus::OutputError(s) => {
                        let m = handles.get_mut(&msg.message_id);
                        match m {
                            None => return Err(Error::ExecutionError("Message ID5 does not exist".into())),
                            Some(state) => {
                                state.output_error_count += 1;
                                state.errors.push(s);
                            },
                        }
                    },
                    MessageStatus::Shutdown => {
                        return Ok(())
                    },
                }
            },
            else => break,
        }
    }
    Ok(())
}

async fn input(
    input: ParsedRegisteredItem,
    output: Sender<InternalMessage>,
    state_handle: Sender<MessageHandle>,
) -> Result<(), Error> {
    trace!("started input");

    let item = (input.creator)(&input.config)?;

    let mut i = match item {
        ExecutionType::Input(i) => i,
        _ => {
            error!("invalid execution type for input");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    debug!("input connected");

    // let mut count = 0;

    loop {
        // if count >= 1000 {
        //     i.close()?;
        //     debug!("input closed");
        //     bus.publish::<ProcessingState>("processing-state", ProcessingState::InputComplete).await.unwrap();
        //     return Ok(());
        // }

        match i.read().await {
            Ok((msg, closure)) => {
                let message_id: String = Uuid::new_v4().into();

                // trace!("message received from input");
                let internal_msg = InternalMessage {
                    message: msg,
                    message_id: message_id.clone(),
                    status: MessageStatus::New,
                };

                state_handle
                    .send(MessageHandle {
                        message_id,
                        closure: Arc::new(closure),
                    })
                    .await
                    .unwrap();

                match output.send(internal_msg).await {
                    Ok(_) => {}
                    Err(e) => return Err(Error::UnableToSendToChannel(format!("{}", e))),
                }
                // count += 1;
            }
            Err(e) => match e {
                Error::EndOfInput => {
                    i.close().await?;
                    debug!("input closed");
                    return Ok(());
                }
                Error::NoInputToReturn => {
                    sleep(Duration::from_millis(1500)).await;
                    continue;
                }
                _ => {
                    i.close().await?;
                    debug!("input closed");
                    error!(error = format!("{}", e), "read error from input");
                    return Err(Error::ExecutionError(format!(
                        "Received error from read: {}",
                        e
                    )));
                }
            },
        }
    }
}
