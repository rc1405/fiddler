use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use std::fmt;
use std::mem;
use tokio::task::JoinSet;
use tracing::{debug, error, info, trace};

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Once;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::CallbackChan;
use super::Error;
use super::Message;
use crate::config::parse_configuration_item;
use crate::config::ExecutionType;
use crate::config::{Config, ItemType, ParsedConfig, ParsedRegisteredItem};

use crate::modules::outputs;
use crate::modules::processors;
use crate::modules::register_plugins;
use crate::MessageType;
use crate::Status;

static REGISTER: Once = Once::new();

/// Represents a single data pipeline configuration Runtime to run
pub struct Runtime {
    config: ParsedConfig,
    state_tx: Sender<InternalMessageState>,
    state_rx: Receiver<InternalMessageState>,
    timeout: Option<Duration>,
}

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub(crate) enum MessageStatus {
    #[default]
    New,
    Processed,
    ProcessError(String),
    Output,
    OutputError(String),
    Shutdown,
    StreamComplete,
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
            MessageStatus::StreamComplete => write!(f, "StreamComplete"),
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

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct InternalMessageState {
    pub message_id: String,
    pub status: MessageStatus,
    pub stream_id: Option<String>,
    pub is_stream: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InternalMessage {
    pub message: Message,
    pub message_id: String,
    pub status: MessageStatus,
}

struct MessageHandle {
    message_id: String,
    closure: Option<CallbackChan>,
    stream_id: Option<String>,
    is_stream: bool,
    stream_complete: bool,
}

impl Runtime {
    /// The function takes the raw configuration of the data pipeline and registers built-in
    /// plugins, validates the configuration and returns the Runtime to run.
    /// ```
    /// use fiddler::Runtime;
    ///
    /// let conf_str = r#"input:
    ///  stdin: {}
    ///processors:
    ///  - label: my_cool_mapping
    ///    noop: {}
    ///output:
    ///  stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// let env = Runtime::from_config(conf_str).await.unwrap();
    /// # })
    /// ```
    pub async fn from_config(config: &str) -> Result<Self, Error> {
        REGISTER.call_once(|| {
            #[allow(clippy::unwrap_used)]
            register_plugins().unwrap();
        });
        trace!("plugins registered");

        let conf: Config = Config::from_str(config)?;
        let parsed_conf = conf.validate().await?;

        let (state_tx, state_rx) = bounded(0);

        debug!("Runtime is ready");
        Ok(Runtime {
            config: parsed_conf,
            state_rx,
            state_tx,
            timeout: None,
        })
    }

    /// The function sets the data pipeline with a label.
    /// ```
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// env.set_label(Some("MyPipeline".into())).unwrap();
    /// # });
    /// ```
    /// or to remove a given label:
    /// ```
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #  stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// env.set_label(None).unwrap();
    /// # });
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
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// # env.set_label(Some("MyPipeline".into())).unwrap();
    /// assert_eq!(env.get_label().unwrap(), "MyPipeline".to_string());
    /// # });
    /// ```
    pub fn get_label(&self) -> Option<String> {
        self.config.label.clone()
    }

    /// The function replaces the existing input configuration with the provided input.
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use std::collections::HashMap;
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// use serde_yaml::Value;
    /// let conf_str = r#"file:
    ///    filename: tests/data/input.txt
    ///    codec: ToEnd"#;
    /// let parsed_input: HashMap<String, Value> = serde_yaml::from_str(conf_str).unwrap();
    ///
    /// env.set_input(&parsed_input).await.unwrap()
    /// # })
    /// ```
    pub async fn set_input(&mut self, input: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Input, input).await?;
        self.config.input = parsed_item;
        Ok(())
    }

    /// The function replaces the existing output configuration with the provided output.
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use std::collections::HashMap;
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    ///
    /// use serde_yaml::Value;
    /// let conf_str = r#"stdout: {}"#;
    /// let parsed_output: HashMap<String, Value> = serde_yaml::from_str(conf_str).unwrap();
    ///
    /// env.set_output(&parsed_output).await.unwrap()
    /// # });
    /// ```
    pub async fn set_output(&mut self, output: &HashMap<String, Value>) -> Result<(), Error> {
        let parsed_item = parse_configuration_item(ItemType::Output, output).await?;
        self.config.output = parsed_item;
        Ok(())
    }

    /// The function sets the number of instances of processors and outputs to create.
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use std::collections::HashMap;
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// env.set_threads(1).unwrap()
    /// # });
    /// ```
    pub fn set_threads(&mut self, count: usize) -> Result<(), Error> {
        self.config.num_threads = count;
        Ok(())
    }

    /// The function sets the timeout, or duration to run the pipeline
    /// ```
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use std::collections::HashMap;
    /// # use std::time::Duration;
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// env.set_timeout(Some(Duration::from_secs(60)))
    /// # });
    /// ```
    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> Result<(), Error> {
        self.timeout = timeout;
        Ok(())
    }
    /// The function runs the existing data pipeline until receiving an Error::EndOfInput
    /// ```no_run
    /// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
    /// # use std::collections::HashMap;
    /// # use fiddler::Runtime;
    /// # let conf_str = r#"input:
    /// #   stdin: {}
    /// # processors:
    /// #  - label: my_cool_mapping
    /// #    noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # tokio_test::block_on(async {
    /// # let mut env = Runtime::from_config(conf_str).await.unwrap();
    /// env.run().await.unwrap();
    /// # })
    /// ```
    pub async fn run(&self) -> Result<(), Error> {
        let mut handles = JoinSet::new();

        let (msg_tx, msg_rx) = bounded(0);
        let msg_state = message_handler(msg_rx, self.state_rx.clone(), self.config.num_threads);

        let _ = handles.spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("state")
                .worker_threads(1)
                // .on_thread_start(move || set_current_thread_priority_low())
                .build()
                .expect("Creating tokio runtime");
            runtime.block_on(msg_state)
        });

        let output = self
            .output(self.config.output.clone(), &mut handles)
            .await?;

        let processors = self.pipeline(output, &mut handles).await?;

        let (ks_send, ks_recv) = bounded(0);

        let input = input(self.config.input.clone(), processors, msg_tx, ks_recv);

        let _ = handles.spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("input")
                .worker_threads(1)
                // .on_thread_start(move || set_current_thread_priority_low())
                .build()
                .expect("Creating tokio runtime");
            runtime.block_on(input)
        });

        info!(label = self.config.label, "pipeline started");

        if let Some(d) = self.timeout {
            let _ = handles.spawn_blocking(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("timeout")
                    .worker_threads(1)
                    // .on_thread_start(move || set_current_thread_priority_low())
                    .build()
                    .expect("Creating tokio runtime");

                runtime.block_on(sleep(d));
                println!("sending kill signal");
                if !ks_send.is_disconnected() {
                    #[allow(clippy::unwrap_used)]
                    ks_send.send(()).unwrap();
                }
                Ok(())
            });
        }

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

        for (i, v) in processors.iter().enumerate() {
            let p = v.clone();

            let (tx, rx) = bounded(0);

            for n in 0..self.config.num_threads {
                // tokio::spawn(processors::run_processor(p.clone(), next_tx.clone(), rx.clone(), self.state_tx.clone()));
                let proc = processors::run_processor(
                    p.clone(),
                    next_tx.clone(),
                    rx.clone(),
                    self.state_tx.clone(),
                );
                let _ = handles.spawn_blocking(move || {
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

        let (tx, rx) = bounded(0);

        for i in 0..self.config.num_threads {
            let item = (output.creator)(output.config.clone()).await?;
            match item {
                ExecutionType::Output(o) => {
                    // handles.spawn(outputs::run_output(rx.clone(), self.state_tx.clone(), o));
                    let state_tx = self.state_tx.clone();
                    let new_rx = rx.clone();
                    let _ = handles.spawn_blocking(move || {
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
                    let _ = handles.spawn_blocking(move || {
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
    closure: Option<CallbackChan>,
    errors: Vec<String>,
    stream_id: Option<String>,
    stream_closed: Option<bool>,
}

fn process_state(
    handles: &mut HashMap<String, State>,
    output_ct: &usize,
    closed_outputs: &mut usize,
    msg: InternalMessageState,
) -> Result<(), Error> {
    // trace!(state = msg.status.to_string(), message_id = msg.message_id, "Received message state");

    let mut remove_entry = false;
    let mut stream_id = None;

    match handles.get_mut(&msg.message_id) {
        None => {
            if let MessageStatus::Shutdown = &msg.status {
                *closed_outputs += 1;
                if closed_outputs == output_ct {
                    info!("exiting message handler");
                    return Err(Error::EndOfInput);
                }
            } else {
                return Err(Error::ExecutionError(format!(
                    "Message ID {} does not exist",
                    msg.message_id
                )));
            };
        }
        Some(state) => {
            match &msg.status {
                MessageStatus::New => {
                    state.instance_count += 1;
                    stream_id = state.stream_id.clone();
                }
                MessageStatus::Processed => {
                    state.processed_count += 1;
                    stream_id = state.stream_id.clone();
                }
                MessageStatus::ProcessError(e) => {
                    state.processed_error_count += 1;
                    state.errors.push(e.clone());
                    stream_id = state.stream_id.clone();

                    let stream_closed = state.stream_closed.unwrap_or(true);

                    if stream_closed
                        && (state.output_count
                            + state.output_error_count
                            + state.processed_error_count)
                            >= state.instance_count
                    {
                        remove_entry = true;
                        if state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let e = Vec::new();
                            let err = mem::replace(&mut state.errors, e);
                            let _ = chan.send(Status::Errored(err));
                        };
                    }
                }
                MessageStatus::Output => {
                    state.output_count += 1;
                    stream_id = state.stream_id.clone();

                    debug!(
                        message_id = msg.message_id,
                        errors = state.processed_error_count,
                        "message fully processed"
                    );
                    let stream_closed = state.stream_closed.unwrap_or(true);

                    if stream_closed && state.output_count >= state.instance_count {
                        remove_entry = true;
                        if state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let _ = chan.send(Status::Processed);
                        };
                    } else if stream_closed
                        && (state.output_count
                            + state.output_error_count
                            + state.processed_error_count)
                            >= state.instance_count
                    {
                        remove_entry = true;
                        if state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let e = Vec::new();
                            let err = mem::replace(&mut state.errors, e);
                            let _ = chan.send(Status::Errored(err));
                        };
                    }
                }
                MessageStatus::OutputError(e) => {
                    state.output_error_count += 1;
                    state.errors.push(e.clone());
                    stream_id = state.stream_id.clone();

                    if (state.output_count + state.output_error_count + state.processed_error_count)
                        >= state.instance_count
                    {
                        remove_entry = state.stream_closed.unwrap_or(true);

                        if remove_entry && state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let e = Vec::new();
                            let err = mem::replace(&mut state.errors, e);
                            let _ = chan.send(Status::Errored(err));
                        };
                    }
                }
                MessageStatus::Shutdown => {
                    *closed_outputs += 1;
                    if closed_outputs == output_ct {
                        debug!("exiting message handler");
                        return Err(Error::EndOfInput);
                    }
                }
                MessageStatus::StreamComplete => {
                    state.stream_closed = Some(true);
                    state.output_count += 1;

                    stream_id = state.stream_id.clone();
                    if state.output_count >= state.instance_count {
                        remove_entry = true;
                        if state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let _ = chan.send(Status::Processed);
                        };
                    } else if (state.output_count
                        + state.output_error_count
                        + state.processed_error_count)
                        >= state.instance_count
                    {
                        remove_entry = true;
                        if state.closure.is_some() {
                            info!(message_id = msg.message_id, "calling closure");
                            let s = None;
                            #[allow(clippy::unwrap_used)]
                            let chan = mem::replace(&mut state.closure, s).unwrap();
                            let e = Vec::new();
                            let err = mem::replace(&mut state.errors, e);
                            let _ = chan.send(Status::Errored(err));
                        };
                    };
                }
            };

            info!(
                instance_count = state.instance_count,
                processed_count = state.processed_count,
                processed_error_count = state.processed_error_count,
                output_count = state.output_count,
                output_error_count = state.output_error_count,
                stream_id = state.stream_id,
                stream_closed = state.stream_closed,
                state = msg.status.to_string(),
                message_id = msg.message_id,
                "Received message state"
            );
        }
    };

    if let Some(stream_id) = stream_id {
        match handles.get(&stream_id) {
            None => {
                return Err(Error::ExecutionError(format!(
                    "StreamID {} does not exist (Message ID {})",
                    stream_id, msg.message_id
                )))
            }
            Some(s) => {
                let mut stream = msg.clone();
                stream.message_id = stream_id.clone();
                stream.stream_id = s.stream_id.clone();
                process_state(handles, output_ct, closed_outputs, stream)?;
            }
        }
    }

    if remove_entry {
        trace!(message_id = msg.message_id, "Removing message from state");
        let _ = handles.remove(&msg.message_id);
    }

    Ok(())
}

async fn message_handler(
    new_msg: Receiver<MessageHandle>,
    msg_status: Receiver<InternalMessageState>,
    output_ct: usize,
) -> Result<(), Error> {
    let mut handles: HashMap<String, State> = HashMap::new();
    let mut closed_outputs = 0;

    loop {
        tokio::select! {
            biased;
            Ok(msg) = new_msg.recv_async() => {
                trace!(message_id = msg.message_id, "Received new message");
                if msg.is_stream && msg.stream_complete {
                    if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, InternalMessageState {
                        message_id: msg.message_id.clone(),
                        status: MessageStatus::StreamComplete,
                        stream_id: msg.stream_id.clone(),
                        is_stream: true
                    }) {
                        match e {
                            Error::EndOfInput => return Ok(()),
                            _ => return Err(e),
                        }
                    }
                    continue
                };

                let closure = msg.closure;
                let i = handles.insert(msg.message_id.clone(), State {
                    instance_count: 1,
                    processed_count: 0,
                    processed_error_count: 0,
                    output_count: 0,
                    output_error_count: 0,
                    closure,
                    errors: Vec::new(),
                    stream_id: msg.stream_id.clone(),
                    stream_closed: match msg.is_stream {
                        true => Some(false),
                        false => None,
                    },
                });
                if i.is_some() {
                    error!(message_id = msg.message_id, "Received duplicate message");
                    return Err(Error::ExecutionError("Duplicate Message ID Error".into()))
                };

                if let Some(s) = &msg.stream_id {
                    if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, InternalMessageState{
                        message_id: s.clone(),
                        status: MessageStatus::New,
                        stream_id: None,
                        is_stream: true,
                    }) {
                        match e {
                            Error::EndOfInput => return Ok(()),
                        _ => return Err(e),
                        }
                    };
                }
            },
            Ok(msg) = msg_status.recv_async() => {
                if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, msg) {
                    match e {
                        Error::EndOfInput => return Ok(()),
                        _ => return Err(e),
                    };
                };
            },
            else => break,
        }
    }

    // add counter and metrics dump how many processed // errored // etc
    Ok(())
}

async fn input(
    input: ParsedRegisteredItem,
    output: Sender<InternalMessage>,
    state_handle: Sender<MessageHandle>,
    kill_switch: Receiver<()>,
) -> Result<(), Error> {
    trace!("started input");

    let item = (input.creator)(input.config.clone()).await?;

    let mut i = match item {
        ExecutionType::Input(i) => i,
        _ => {
            error!("invalid execution type for input");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    debug!("input connected");

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
                        let message_type = msg.message_type.clone();

                        let message_id: String = match &message_type {
                            MessageType::Default => Uuid::new_v4().into(),
                            MessageType::BeginStream(id) => id.clone(),
                            MessageType::EndStream(id) => id.clone(),
                        };

                        debug!(message_id = message_id, message_type = format!("{:?}", message_type), "received message");

                        let is_stream = match &message_type {
                            MessageType::BeginStream(_) => true,
                            MessageType::EndStream(_) => true,
                            MessageType::Default => false,
                        };
                        state_handle
                            .send_async(MessageHandle {
                                message_id: message_id.clone(),
                                closure,
                                stream_id: msg.stream_id.clone(),
                                is_stream,
                                stream_complete: matches!(&message_type, MessageType::EndStream(_))
                            })
                            .await
                            .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;

                        // if message.type != registration forward down the pipeline
                        // new input send main event with type parent or whatever with a callback
                        // for sqs, delete main message
                        // each subsequent message adds in an Option<String> for the parentId
                        // in state if item has a parent ID // increment or decrement the parent's counter
                        // if parent counter is zero, then send aggregated status
                        if let MessageType::Default = message_type {
                            let internal_msg = InternalMessage {
                                message: msg,
                                message_id: message_id.clone(),
                                status: MessageStatus::New,
                            };

                            output
                                .send_async(internal_msg)
                                .await
                                .map_err(|e| Error::UnableToSendToChannel(format!("{}", e)))?;
                        }

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
                            println!("failure");
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
            },
        }
    }
}
