use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{Sender, Receiver, channel};
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use tokio::task::yield_now;
use tokio::task::JoinSet;
use tokio::sync::oneshot;
use std::mem;
use std::fmt;
use tracing::{debug, error, info, trace};

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Once;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

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

static REGISTER: Once = Once::new();

/// Represents a single data pipeline configuration Runtime to run
pub struct Runtime {
    config: ParsedConfig,
}

#[derive(Clone, Serialize, Deserialize, Default)]
enum MessageStatus {
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
enum ProcessingState {
    #[default]
    InputComplete,
    ProcessorSubscribe((usize, String)),
    ProcessorComplete(usize),
    PipelineComplete,
    OutputSubscribe(String),
    OutputComplete,
}

#[derive(Clone, Serialize, Deserialize)]
struct InternalMessageState {
    message_id: String,
    status: MessageStatus,
}

#[derive(Clone, Serialize, Deserialize)]
struct InternalMessage {
    message: Message,
    message_id: String,
    status: MessageStatus,
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

        debug!("Runtime is ready");
        Ok(Runtime {
            config: parsed_conf,
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
    /// # pipeline:
    /// #   processors:
    /// #    - label: my_cool_mapping
    /// #      noop: {}
    /// # output:
    /// #   stdout: {}"#;
    /// # let mut env = Runtime::from_config(conf_str).unwrap();
    /// # tokio_test::block_on(async {
    /// env.run().await.unwrap();
    /// # })
    /// ```
    pub async fn run(&self) -> Result<(), Error> {
        let bus = fiddler_bus::EventBus::new().await.unwrap();
        bus.create_topic("input", Some(1)).await.unwrap();
        bus.create_topic("message-state", Some(1)).await.unwrap();
        bus.create_topic("processing-state", None).await.unwrap();
        for n in 0..self.config.pipeline.processors.len() {
            bus.create_topic(&format!("processor{}", n), Some(1)).await.unwrap()
        }
        bus.create_topic("output", Some(1)).await.unwrap();
        let (state_sender, state_receiver) = channel(1);

        let barrier = Arc::new(Notify::new());

        let mut handles = JoinSet::new();

        let msg_state = message_handler(state_receiver, bus.clone());
        handles.spawn(msg_state);

        let state = state_handler(bus.clone(), self.config.pipeline.processors.len(), barrier.clone());
        handles.spawn(state);
       
        let output = output(bus.clone(), self.config.output.clone());
        handles.spawn(output);

        let p = pipeline(
            self.config.pipeline.clone(),
            bus.clone(),
        );
        handles.spawn(p);

        let input = input(
            self.config.input.clone(),
            bus.clone(),
            state_sender,
            barrier,
        );

        info!("pipeline started");
        handles.spawn(input);

        while let Some(res) = handles.join_next().await {
            res.map_err(|e| Error::ProcessingError(format!("{}", e)))??;
        }

        info!("pipeline finished");
        Ok(())
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

// processor subs not in place when input finishes.

async fn state_handler(
    bus: fiddler_bus::EventBus::<fiddler_bus::Connected>,
    processor_count: usize,
    barrier: Arc<Notify>,
) -> Result<(), Error> {
    let (proc_id, mut proc_topic) = bus.subscribe::<ProcessingState>("processing-state").await.unwrap();
    let mut processor_subs: HashMap<usize, String> = HashMap::new();
    let mut output_handle: Option<String> = None;
    
    

    loop {
        tokio::select! {
            Some(msg) = proc_topic.recv() => {
                match msg {
                    ProcessingState::InputComplete => {
                        trace!("Received input complete state");
                        let b = bus.clone();

                        let i = processor_subs.get(&0);
                        if let Some(id) = i {
                            let rid = id.clone();
                            tokio::spawn(async move {
                                while b.count::<InternalMessage>("processor0").await.unwrap() > 0 {
                                    sleep(Duration::from_secs(5)).await
                                };
        
                                trace!(topic = "processor0", "closing subscription");
                                b.unsubscribe(&rid).await.unwrap();
                            });
                        } else {
                            error!("no processor to unsubscribe");
                        };

                    },
                    ProcessingState::ProcessorSubscribe((i, id)) => {
                        trace!(index = i, subscription_id = id, "Received processor subscribe state");
                        processor_subs.insert(i, id);

                        if processor_subs.len() == processor_count && output_handle.is_some() {
                            trace!("notifying input to start");
                            barrier.notify_one();
                        };
                        
                    },
                    ProcessingState::ProcessorComplete(i) => {
                        trace!(index = i, "Received processor complete state");
                        let index = i+1;
                        let topic = format!("processor{}", index);
                        let b = bus.clone();

                        let p = processor_subs.get(&index);
                        if let Some(id) = p {
                            let rid = id.clone();
                            tokio::spawn(async move {
                                while b.count::<InternalMessage>(&topic).await.unwrap() > 0 {
                                    sleep(Duration::from_secs(5)).await
                                };
                                trace!(topic = topic, "closing subscription");
                                b.unsubscribe(&rid).await.unwrap();
                            });
                        } else {
                            error!("no processor to unsubscribe");
                        }
                        
                    },
                    ProcessingState::PipelineComplete => {
                        trace!("Received pipeline complete state");
                        if let Some(ref id) = output_handle {
                            let rid = id.clone();
                            let b = bus.clone();
                            tokio::spawn(async move {
                                while b.count::<InternalMessage>("output").await.unwrap() > 0 {
                                    sleep(Duration::from_secs(5)).await
                                };
                                trace!(topic = "output", "closing subscription");
                                b.unsubscribe(&rid).await.unwrap();
                            });
                        } else {
                            error!("no output to unsubscribe");
                        }
                    },
                    ProcessingState::OutputSubscribe(id) => {
                        trace!(subscription_id = id, "Received output subscribe state");
                        output_handle = Some(id);
                        if processor_subs.len() == processor_count && output_handle.is_some() {
                            trace!("notifying input to start");
                            barrier.notify_one();
                        };
                    },
                    ProcessingState::OutputComplete => {
                        trace!("Received output complete state");
                        bus.unsubscribe(&proc_id).await.unwrap();
                        return Ok(())
                    },
                }
            },
            else => break,
        }
    }
    Ok(())
}

async fn message_handler(
    mut input: Receiver<MessageHandle>,
    bus: fiddler_bus::EventBus::<fiddler_bus::Connected>
) -> Result<(), Error> {
    let (sub_id, mut msg_topic) = bus.subscribe::<InternalMessageState>("message-state").await.unwrap();
    let mut handles: HashMap<String, State> = HashMap::new();

    loop {
        tokio::select! {
            Some(msg) = input.recv() => {
                trace!(message_id = msg.message_id, "Received new message");
                let closure = Arc::into_inner(msg.closure).unwrap();
                let i = handles.insert(msg.message_id.clone(), State { instance_count: 1, processed_count: 0, processed_error_count: 0, output_count: 0, output_error_count: 0, closure: closure, errors: Vec::new() });
                if i.is_some() {
                    error!(message_id = msg.message_id, "Received duplicate message");
                    return Err(Error::ExecutionError("Duplicate Message ID Error".into()))
                };
            },
            Some(msg) = msg_topic.recv() => {
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
                        bus.unsubscribe(&sub_id).await.unwrap();
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
    bus: fiddler_bus::EventBus<fiddler_bus::Connected>,
    state_handle: Sender<MessageHandle>,
    barrier: Arc<Notify>,
) -> Result<(), Error> {
    trace!("started input");
    let i = match &input.execution_type {
        ExecutionType::Input(i) => i,
        _ => {
            error!("invalid execution type for input");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    i.connect().await?;
    debug!("input connected");

    barrier.notified().await;

    loop {
        match i.read().await {
            Ok((msg, closure)) => {
                let message_id: String = Uuid::new_v4().into();

                trace!("message received from input");
                let internal_msg = InternalMessage {
                    message: msg,
                    message_id: message_id.clone(),
                    status: MessageStatus::New,
                };

                state_handle.send(MessageHandle{
                    message_id,
                    closure: Arc::new(closure),
                }).await.unwrap();

                bus.publish::<InternalMessage>("input", internal_msg).await.unwrap();
            }
            Err(e) => match e {
                Error::EndOfInput => {
                    i.close()?;
                    debug!("input closed");
                    bus.publish::<ProcessingState>("processing-state", ProcessingState::InputComplete).await.unwrap();
                    return Ok(());
                }
                Error::NoInputToReturn => {
                    sleep(Duration::from_millis(1500)).await;
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
            },
        }
    }
}

async fn pipeline(
    pipeline: ParsedPipeline,
    bus: fiddler_bus::EventBus<fiddler_bus::Connected>,
) -> Result<(), Error> {
    trace!("starting pipeline");
    
    let mut pipelines = JoinSet::new();

    for i in 0..pipeline.processors.len() {
        let p = pipeline.processors[i].clone();
        let subscription = match i {
            0 => "input".into(),
            _ => format!("processor{}", i-1),
        };

        let publisher = if (i+1) == pipeline.processors.len() {
            "output".into()
        } else {
            format!("processor{}", i)
        };

        let b = bus.clone();
        pipelines.spawn(async move { run_processor(b, subscription, publisher, p, i).await });
    }

    while let Some(res) = pipelines.join_next().await {
        res.map_err(|e| Error::ProcessingError(format!("{}", e)))??;
    }

    debug!("shutting down pipeline");
    bus.publish::<ProcessingState>("processing-state", ProcessingState::PipelineComplete).await.unwrap();

    Ok(())
}

async fn run_processor(
    bus: fiddler_bus::EventBus<fiddler_bus::Connected>,
    subscription: String,
    publisher: String,
    processor: ParsedRegisteredItem,
    id: usize,
) -> Result<(), Error> {
    trace!(subscribed_to = subscription, publish_to = publisher, "Started processor");
    let p = match &processor.execution_type {
        ExecutionType::Processor(p) => p,
        _ => {
            error!("invalid execution type for processor");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    let (sub_id, mut input) = bus.subscribe::<InternalMessage>(&subscription).await.unwrap();
    bus.publish::<ProcessingState>("processing-state", ProcessingState::ProcessorSubscribe((id, sub_id))).await.unwrap();

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
                                bus.publish::<InternalMessageState>("message-state", InternalMessageState{
                                    message_id: msg.message_id.clone(),
                                    status: MessageStatus::New,
                                }).await.unwrap();
                            };
                            trace!("message processed");
                            bus.publish::<InternalMessage>(&publisher, new_msg).await.unwrap();
                        }
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for processor");

                            bus.publish::<InternalMessageState>("message-state", InternalMessageState{
                                message_id: msg.message_id,
                                status: MessageStatus::ProcessError(format!("{}", e)),
                            }).await.unwrap();
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
                    bus.publish::<ProcessingState>("processing-state", ProcessingState::ProcessorComplete(id)).await.unwrap();
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
        yield_now().await;
    }
}

async fn output(
    bus: fiddler_bus::EventBus<fiddler_bus::Connected>,
    output: ParsedRegisteredItem,
) -> Result<(), Error> {
    trace!("started output");
    let o = match &output.execution_type {
        ExecutionType::Output(o) => o,
        _ => {
            error!("invalid execution type for output");
            return Err(Error::Validation("invalid execution type".into()));
        }
    };

    o.connect().await?;
    debug!("output connected");

    let (sub_id, mut input) = bus.subscribe::<InternalMessage>("output").await.unwrap();
    bus.publish::<ProcessingState>("processing-state", ProcessingState::OutputSubscribe(sub_id)).await.unwrap();

    loop {
        match input.try_recv() {
            Ok(msg) => {
                trace!("received output message");
                match o.write(msg.message.clone()).await {
                    Ok(_) => {
                        bus.publish::<InternalMessageState>("message-state", InternalMessageState{
                            message_id: msg.message_id,
                            status: MessageStatus::Output,
                        }).await.unwrap();
                    }
                    Err(e) => match e {
                        Error::ConditionalCheckfailed => {
                            debug!("conditional check failed for output");
                        }
                        _ => {
                            bus.publish::<InternalMessageState>("message-state", InternalMessageState{
                                message_id: msg.message_id,
                                status: MessageStatus::OutputError(format!("{}", e)),
                            }).await.unwrap();
                        }
                    },
                }
            }
            Err(e) => match e {
                TryRecvError::Disconnected => {
                    o.close()?;
                    debug!("output closed");
                    bus.publish::<ProcessingState>("processing-state", ProcessingState::OutputComplete).await.unwrap();
                    bus.publish::<InternalMessageState>("message-state", InternalMessageState{
                        message_id: "end of the line".into(),
                        status: MessageStatus::Shutdown,
                    }).await.unwrap();
                    return Ok(());
                }
                TryRecvError::Empty => sleep(Duration::from_millis(250)).await,
            },
        };
    }
}
