use flume::{bounded, Receiver, Sender};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{interval, sleep, Duration, MissedTickBehavior};
use tracing::{debug, error, info, trace, warn};

use crate::MetricEntry;
use std::str::FromStr;
use std::sync::Once;
use uuid::Uuid;

/// Minimum backoff duration when no input is available (in microseconds)
const NO_INPUT_BACKOFF_MIN_US: u64 = 1;

/// Maximum backoff duration when no input is available (in milliseconds)
const NO_INPUT_BACKOFF_MAX_MS: u64 = 10;

/// Timeout for stale message entries in the state tracker (1 hour)
const STALE_MESSAGE_TIMEOUT_SECS: u64 = 3600;

/// Interval for cleaning up stale entries (5 minutes)
const STALE_CLEANUP_INTERVAL_SECS: u64 = 300;

/// Default capacity for internal message channels.
/// Higher values allow more buffering and parallelism but use more memory.
const CHANNEL_CAPACITY: usize = 10_000;

/// Tracks message processing statistics for observability.
///
/// This struct maintains counters for various message processing events
/// to enable monitoring and debugging of the pipeline.
#[derive(Debug, Default)]
pub struct MessageMetrics {
    /// Total messages received from input
    pub total_received: u64,
    /// Messages successfully processed through all outputs
    pub total_completed: u64,
    /// Messages that encountered processing errors
    pub total_process_errors: u64,
    /// Messages that encountered output errors
    pub total_output_errors: u64,
    /// Streams started
    pub streams_started: u64,
    /// Streams completed
    pub streams_completed: u64,
    /// Duplicate messages rejected
    pub duplicates_rejected: u64,
    /// Stale entries cleaned up
    pub stale_entries_removed: u64,
    /// Timestamp when metrics collection started
    started_at: Option<Instant>,
}

impl MessageMetrics {
    /// Creates a new MessageMetrics instance with the current timestamp.
    pub fn new() -> Self {
        Self {
            started_at: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Returns the duration since metrics collection started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Returns the throughput in messages per second.
    pub fn throughput_per_sec(&self) -> f64 {
        let elapsed = self.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_completed as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Returns the error rate as a percentage.
    pub fn error_rate(&self) -> f64 {
        let total = self.total_completed + self.total_process_errors + self.total_output_errors;
        if total > 0 {
            ((self.total_process_errors + self.total_output_errors) as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Records metrics to the configured metrics backend.
    ///
    /// If no metrics backend is configured, this is a no-op.
    pub fn record(&self, metrics_backend: &mut dyn Metrics, in_flight: usize) {
        metrics_backend.record(MetricEntry {
            total_received: self.total_received,
            total_completed: self.total_completed,
            total_process_errors: self.total_process_errors,
            total_output_errors: self.total_output_errors,
            streams_started: self.streams_started,
            streams_completed: self.streams_completed,
            duplicates_rejected: self.duplicates_rejected,
            stale_entries_removed: self.stale_entries_removed,
            in_flight: in_flight,
            throughput_per_sec: self.throughput_per_sec(),
        });
    }
}

/// Spawns an async task on the shared tokio runtime.
/// Using the shared runtime enables work-stealing across all tasks for better CPU utilization.
fn spawn_task<F>(handles: &mut JoinSet<Result<(), Error>>, task: F)
where
    F: Future<Output = Result<(), Error>> + Send + 'static,
{
    handles.spawn(task);
}

use super::CallbackChan;
use super::Error;
use super::Message;
use super::Metrics;
use crate::config::parse_configuration_item;
use crate::config::ExecutionType;
use crate::config::{Config, ItemType, ParsedConfig, ParsedRegisteredItem};

use crate::modules::metrics::create_metrics;
use crate::modules::outputs;
use crate::modules::processors;
use crate::modules::register_plugins;
use crate::MessageType;
use crate::Status;

use once_cell::sync::Lazy;
use std::sync::Mutex;

static REGISTER: Once = Once::new();
/// Stores any error that occurred during plugin registration
static REGISTER_ERROR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

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
            if let Err(e) = register_plugins() {
                if let Ok(mut err) = REGISTER_ERROR.lock() {
                    *err = Some(format!("{e}"));
                }
            }
        });

        // Check if registration failed
        if let Ok(err_lock) = REGISTER_ERROR.lock() {
            if let Some(ref e) = *err_lock {
                return Err(Error::ExecutionError(format!(
                    "Plugin registration failed: {e}"
                )));
            }
        }
        trace!("plugins registered");

        let conf: Config = Config::from_str(config)?;
        let parsed_conf = conf.validate().await?;

        let (state_tx, state_rx) = bounded(CHANNEL_CAPACITY);

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

        // Create metrics backend based on configuration
        let metrics_backend = create_metrics(self.config.metrics.as_ref()).await?;

        // Get metrics recording interval from config, or use default (30 seconds)
        let metrics_interval = self
            .config
            .metrics
            .as_ref()
            .map(|m| m.interval)
            .unwrap_or(30);

        let (msg_tx, msg_rx) = bounded(CHANNEL_CAPACITY);
        let msg_state = message_handler(
            msg_rx,
            self.state_rx.clone(),
            self.config.num_threads,
            metrics_backend,
            metrics_interval,
        );

        spawn_task(&mut handles, msg_state);

        let output = self
            .output(self.config.output.clone(), &mut handles)
            .await?;

        let processors = self.pipeline(output, &mut handles).await?;

        // Kill switch only needs capacity of 1 - it's a signal channel
        let (ks_send, ks_recv) = bounded(1);

        let input = input(self.config.input.clone(), processors, msg_tx, ks_recv);

        spawn_task(&mut handles, input);

        info!(label = self.config.label, "pipeline started");

        if let Some(d) = self.timeout {
            let timeout_ks_send = ks_send.clone();
            handles.spawn(async move {
                sleep(d).await;
                trace!("sending kill signal");
                if !timeout_ks_send.is_disconnected() {
                    if let Err(e) = timeout_ks_send.send(()) {
                        debug!(error = ?e, "Failed to send kill signal, receiver may have been dropped");
                    }
                }
                Ok(())
            });
        }

        // Main loop: wait for tasks to complete or Ctrl+C signal
        loop {
            tokio::select! {
                // Handle task completion
                res = handles.join_next() => {
                    match res {
                        Some(Ok(Ok(()))) => {
                            // Task completed successfully, continue waiting for others
                        }
                        Some(Ok(Err(e))) => {
                            // Task returned an error
                            return Err(e);
                        }
                        Some(Err(e)) => {
                            // Task panicked or was cancelled
                            return Err(Error::ProcessingError(format!("{e}")));
                        }
                        None => {
                            // All tasks completed
                            break;
                        }
                    }
                }
                // Handle Ctrl+C signal for graceful shutdown
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal (Ctrl+C), initiating graceful shutdown");
                    if !ks_send.is_disconnected() {
                        if let Err(e) = ks_send.send(()) {
                            debug!(error = ?e, "Failed to send kill signal from signal handler");
                        }
                    }
                    // Continue the loop to let tasks shut down gracefully
                }
            }
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

            let (tx, rx) = bounded(CHANNEL_CAPACITY);

            for n in 0..self.config.num_threads {
                let proc = processors::run_processor(
                    p.clone(),
                    next_tx.clone(),
                    rx.clone(),
                    self.state_tx.clone(),
                );
                spawn_task(handles, proc);
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

        let (tx, rx) = bounded(CHANNEL_CAPACITY);

        for i in 0..self.config.num_threads {
            let item = (output.creator)(output.config.clone()).await?;
            match item {
                ExecutionType::Output(o) => {
                    let state_tx = self.state_tx.clone();
                    let new_rx = rx.clone();
                    spawn_task(handles, outputs::run_output(new_rx, state_tx, o));
                }
                ExecutionType::OutputBatch(o) => {
                    let state_tx = self.state_tx.clone();
                    let new_rx = rx.clone();
                    spawn_task(handles, outputs::run_output_batch(new_rx, state_tx, o));
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
    /// Timestamp when this state was created, used for stale entry cleanup
    created_at: Instant,
}

/// Process state updates iteratively to avoid stack overflow with deeply nested streams
fn process_state(
    handles: &mut FxHashMap<String, State>,
    output_ct: &usize,
    closed_outputs: &mut usize,
    initial_msg: InternalMessageState,
    metrics: &mut MessageMetrics,
) -> Result<(), Error> {
    // Use a stack for iterative processing instead of recursion
    // Pre-allocate with realistic capacity to avoid reallocation
    let mut pending_messages = Vec::with_capacity(4);
    pending_messages.push(initial_msg);
    let mut entries_to_remove = Vec::with_capacity(2);

    while let Some(msg) = pending_messages.pop() {
        let mut remove_entry = false;
        let mut stream_id = None;
        let mut message_completed_successfully = false;

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
                        metrics.total_process_errors += 1;

                        let stream_closed = state.stream_closed.unwrap_or(true);

                        if stream_closed
                            && (state.output_count
                                + state.output_error_count
                                + state.processed_error_count)
                                >= state.instance_count
                        {
                            remove_entry = true;
                            if let Some(chan) = state.closure.take() {
                                info!(message_id = msg.message_id, "calling closure");
                                let err = std::mem::take(&mut state.errors);
                                let _ = chan.send(Status::Errored(err));
                            }
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
                            message_completed_successfully = true;
                            if let Some(chan) = state.closure.take() {
                                info!(message_id = msg.message_id, "calling closure");
                                let _ = chan.send(Status::Processed);
                            }
                        } else if stream_closed
                            && (state.output_count
                                + state.output_error_count
                                + state.processed_error_count)
                                >= state.instance_count
                        {
                            remove_entry = true;
                            if let Some(chan) = state.closure.take() {
                                info!(message_id = msg.message_id, "calling closure");
                                let err = std::mem::take(&mut state.errors);
                                let _ = chan.send(Status::Errored(err));
                            }
                        }
                    }
                    MessageStatus::OutputError(e) => {
                        state.output_error_count += 1;
                        state.errors.push(e.clone());
                        stream_id = state.stream_id.clone();
                        metrics.total_output_errors += 1;

                        if (state.output_count
                            + state.output_error_count
                            + state.processed_error_count)
                            >= state.instance_count
                        {
                            remove_entry = state.stream_closed.unwrap_or(true);

                            if remove_entry {
                                if let Some(chan) = state.closure.take() {
                                    info!(message_id = msg.message_id, "calling closure");
                                    let err = std::mem::take(&mut state.errors);
                                    let _ = chan.send(Status::Errored(err));
                                }
                            }
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
                            message_completed_successfully = true;
                            if let Some(chan) = state.closure.take() {
                                info!(message_id = msg.message_id, "calling closure");
                                let _ = chan.send(Status::Processed);
                            }
                        } else if (state.output_count
                            + state.output_error_count
                            + state.processed_error_count)
                            >= state.instance_count
                        {
                            remove_entry = true;
                            if let Some(chan) = state.closure.take() {
                                info!(message_id = msg.message_id, "calling closure");
                                let err = std::mem::take(&mut state.errors);
                                let _ = chan.send(Status::Errored(err));
                            }
                        };
                    }
                };

                trace!(
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

        // Queue stream state update instead of recursive call
        if let Some(sid) = stream_id {
            match handles.get(&sid) {
                None => {
                    return Err(Error::ExecutionError(format!(
                        "StreamID {} does not exist (Message ID {})",
                        sid, msg.message_id
                    )))
                }
                Some(s) => {
                    pending_messages.push(InternalMessageState {
                        message_id: sid,
                        status: msg.status.clone(),
                        stream_id: s.stream_id.clone(),
                        is_stream: true,
                    });
                }
            }
        }

        if remove_entry {
            trace!(
                message_id = msg.message_id,
                "Marking message for removal from state"
            );
            entries_to_remove.push(msg.message_id);
            // Track completed messages (only count non-stream messages to avoid double counting)
            if message_completed_successfully && !msg.is_stream {
                metrics.total_completed += 1;
            }
        }
    }

    // Remove entries after processing to avoid borrow conflicts
    for message_id in entries_to_remove {
        let _ = handles.remove(&message_id);
    }

    Ok(())
}

async fn message_handler(
    new_msg: Receiver<MessageHandle>,
    msg_status: Receiver<InternalMessageState>,
    output_ct: usize,
    mut metrics_backend: Box<dyn Metrics>,
    metrics_interval_secs: u64,
) -> Result<(), Error> {
    // Pre-allocate FxHashMap for expected concurrent messages (faster than SipHash)
    let mut handles: FxHashMap<String, State> = FxHashMap::default();
    handles.reserve(1024);
    let mut closed_outputs = 0;
    let stale_timeout = Duration::from_secs(STALE_MESSAGE_TIMEOUT_SECS);
    let mut metrics = MessageMetrics::new();

    // Use tokio interval timers instead of manual elapsed() checks
    let mut metrics_timer = interval(Duration::from_secs(metrics_interval_secs));
    metrics_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Skip the first immediate tick
    metrics_timer.tick().await;

    let mut cleanup_timer = interval(Duration::from_secs(STALE_CLEANUP_INTERVAL_SECS));
    cleanup_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Skip the first immediate tick
    cleanup_timer.tick().await;

    debug!(
        interval_secs = metrics_interval_secs,
        "Metrics recording interval configured"
    );

    loop {
        tokio::select! {
            // biased ensures new messages are registered before status updates are processed
            biased;
            Ok(msg) = new_msg.recv_async() => {
                trace!(message_id = msg.message_id, "Received new message");
                if msg.is_stream && msg.stream_complete {
                    metrics.streams_completed += 1;
                    if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, InternalMessageState {
                        message_id: msg.message_id.clone(),
                        status: MessageStatus::StreamComplete,
                        stream_id: msg.stream_id.clone(),
                        is_stream: true
                    }, &mut metrics) {
                        match e {
                            Error::EndOfInput => {
                                log_shutdown_metrics(&metrics, handles.len(), metrics_backend.as_mut());
                                return Ok(());
                            }
                            _ => return Err(e),
                        }
                    }
                    continue
                };

                // Track new message received
                metrics.total_received += 1;
                if msg.is_stream {
                    metrics.streams_started += 1;
                }

                let closure = msg.closure;
                let stream_id = msg.stream_id;
                let is_stream = msg.is_stream;
                // Consume message_id instead of cloning - Entry API takes ownership
                match handles.entry(msg.message_id) {
                    Entry::Vacant(entry) => {
                        entry.insert(State {
                            instance_count: 1,
                            processed_count: 0,
                            processed_error_count: 0,
                            output_count: 0,
                            output_error_count: 0,
                            closure,
                            errors: Vec::with_capacity(2), // Pre-allocate for common case
                            stream_id: stream_id.clone(),
                            stream_closed: if is_stream { Some(false) } else { None },
                            created_at: Instant::now(),
                        });
                    }
                    Entry::Occupied(entry) => {
                        metrics.duplicates_rejected += 1;
                        error!(message_id = entry.key(), "Received duplicate message");
                        return Err(Error::ExecutionError("Duplicate Message ID Error".into()));
                    }
                }

                if let Some(s) = &stream_id {
                    if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, InternalMessageState{
                        message_id: s.clone(),
                        status: MessageStatus::New,
                        stream_id: None,
                        is_stream: true,
                    }, &mut metrics) {
                        match e {
                            Error::EndOfInput => {
                                log_shutdown_metrics(&metrics, handles.len(), metrics_backend.as_mut());
                                return Ok(());
                            }
                        _ => return Err(e),
                        }
                    };
                }
            },
            Ok(msg) = msg_status.recv_async() => {
                if let Err(e) = process_state(&mut handles, &output_ct, &mut closed_outputs, msg, &mut metrics) {
                    match e {
                        Error::EndOfInput => {
                            log_shutdown_metrics(&metrics, handles.len(), metrics_backend.as_mut());
                            return Ok(());
                        }
                        _ => return Err(e),
                    };
                };
            },
            _ = metrics_timer.tick() => {
                metrics.record(metrics_backend.as_mut(), handles.len());
                trace!(
                    in_flight = handles.len(),
                    throughput = format!("{:.2}", metrics.throughput_per_sec()),
                    "Recorded periodic metrics"
                );
            },
            _ = cleanup_timer.tick() => {
                let before_count = handles.len();
                handles.retain(|message_id, state| {
                    let is_stale = state.created_at.elapsed() >= stale_timeout;
                    if is_stale {
                        warn!(
                            message_id = message_id,
                            age_secs = state.created_at.elapsed().as_secs(),
                            "Removing stale message state entry"
                        );
                    }
                    !is_stale
                });
                let removed = before_count - handles.len();
                if removed > 0 {
                    metrics.stale_entries_removed += removed as u64;
                    info!(
                        removed_count = removed,
                        remaining_count = handles.len(),
                        "Cleaned up stale message state entries"
                    );
                }
            },
            else => break,
        }
    }

    log_shutdown_metrics(&metrics, handles.len(), metrics_backend.as_mut());
    Ok(())
}

/// Logs comprehensive shutdown metrics for observability.
fn log_shutdown_metrics(
    metrics: &MessageMetrics,
    in_flight: usize,
    metrics_backend: &mut dyn Metrics,
) {
    // Record final metrics to configured backend
    metrics.record(metrics_backend, in_flight);

    info!(
        total_received = metrics.total_received,
        total_completed = metrics.total_completed,
        total_process_errors = metrics.total_process_errors,
        total_output_errors = metrics.total_output_errors,
        streams_started = metrics.streams_started,
        streams_completed = metrics.streams_completed,
        duplicates_rejected = metrics.duplicates_rejected,
        stale_entries_removed = metrics.stale_entries_removed,
        duration_secs = metrics.elapsed().as_secs(),
        throughput_per_sec = format!("{:.2}", metrics.throughput_per_sec()),
        error_rate_percent = format!("{:.2}", metrics.error_rate()),
        remaining_in_flight = in_flight,
        "Message handler shutdown complete"
    );
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

    // Track consecutive no-input errors for exponential backoff
    let mut no_input_count: u32 = 0;

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
                        // Reset backoff on successful read
                        no_input_count = 0;
                        let message_type = msg.message_type.clone();

                        let message_id: String = match &message_type {
                            MessageType::Default => Uuid::new_v4().into(),
                            MessageType::BeginStream(id) => id.clone(),
                            MessageType::EndStream(id) => id.clone(),
                        };

                        trace!(message_id = message_id, message_type = format!("{:?}", message_type), "received message");

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
                            .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;

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
                                .map_err(|e| Error::UnableToSendToChannel(format!("{e}")))?;
                        }

                    }
                    Err(e) => match e {
                        Error::EndOfInput => {
                            i.close().await?;
                            debug!("input closed");
                            return Ok(());
                        }
                        Error::NoInputToReturn => {
                            // Exponential backoff: 1μs, 2μs, 4μs, ..., up to 10ms
                            let backoff_us = NO_INPUT_BACKOFF_MIN_US
                                .saturating_mul(1u64 << no_input_count.min(20))
                                .min(NO_INPUT_BACKOFF_MAX_MS * 1000);
                            sleep(Duration::from_micros(backoff_us)).await;
                            no_input_count = no_input_count.saturating_add(1);
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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_metrics_new() {
        let metrics = MessageMetrics::new();
        assert_eq!(metrics.total_received, 0);
        assert_eq!(metrics.total_completed, 0);
        assert_eq!(metrics.total_process_errors, 0);
        assert_eq!(metrics.total_output_errors, 0);
        assert_eq!(metrics.streams_started, 0);
        assert_eq!(metrics.streams_completed, 0);
        assert_eq!(metrics.duplicates_rejected, 0);
        assert_eq!(metrics.stale_entries_removed, 0);
        assert!(metrics.started_at.is_some());
    }

    #[test]
    fn test_message_metrics_default() {
        let metrics = MessageMetrics::default();
        assert_eq!(metrics.total_received, 0);
        assert!(metrics.started_at.is_none());
    }

    #[test]
    fn test_message_metrics_elapsed() {
        let metrics = MessageMetrics::new();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = metrics.elapsed();
        assert!(elapsed.as_millis() >= 10);
    }

    #[test]
    fn test_message_metrics_elapsed_without_start() {
        let metrics = MessageMetrics::default();
        let elapsed = metrics.elapsed();
        assert_eq!(elapsed, std::time::Duration::default());
    }

    #[test]
    fn test_message_metrics_throughput_per_sec() {
        let mut metrics = MessageMetrics::new();
        metrics.total_completed = 100;
        // Throughput depends on elapsed time, just verify it returns a value
        let throughput = metrics.throughput_per_sec();
        assert!(throughput >= 0.0);
    }

    #[test]
    fn test_message_metrics_throughput_zero_elapsed() {
        let mut metrics = MessageMetrics::default();
        metrics.total_completed = 100;
        // With no start time, elapsed is 0, so throughput should be 0
        let throughput = metrics.throughput_per_sec();
        assert_eq!(throughput, 0.0);
    }

    #[test]
    fn test_message_metrics_error_rate_no_messages() {
        let metrics = MessageMetrics::new();
        let error_rate = metrics.error_rate();
        assert_eq!(error_rate, 0.0);
    }

    #[test]
    fn test_message_metrics_error_rate_with_errors() {
        let mut metrics = MessageMetrics::new();
        metrics.total_completed = 90;
        metrics.total_process_errors = 5;
        metrics.total_output_errors = 5;
        let error_rate = metrics.error_rate();
        // 10 errors out of 100 total = 10%
        assert!((error_rate - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_message_metrics_error_rate_all_errors() {
        let mut metrics = MessageMetrics::new();
        metrics.total_completed = 0;
        metrics.total_process_errors = 50;
        metrics.total_output_errors = 50;
        let error_rate = metrics.error_rate();
        // 100 errors out of 100 total = 100%
        assert!((error_rate - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_message_metrics_record_with_noop_backend() {
        use crate::modules::metrics::NoOpMetrics;
        // Should work with no-op metrics backend
        let metrics = MessageMetrics::new();
        let mut backend = NoOpMetrics::new();
        metrics.record(&mut backend, 10);
        // No assertion needed - just verify it doesn't panic
    }

    #[test]
    fn test_process_state_unknown_message_id() {
        let mut handles = FxHashMap::default();
        let mut closed_outputs = 0;
        let mut metrics = MessageMetrics::new();

        // Try to process a status for a non-existent message
        let result = process_state(
            &mut handles,
            &1,
            &mut closed_outputs,
            InternalMessageState {
                message_id: "unknown_id".to_string(),
                status: MessageStatus::Processed,
                stream_id: None,
                is_stream: false,
            },
            &mut metrics,
        );

        // Should return an error for unknown message ID
        assert!(result.is_err());
        match result {
            Err(Error::ExecutionError(msg)) => {
                assert!(msg.contains("does not exist"));
            }
            _ => panic!("Expected ExecutionError"),
        }
    }

    #[test]
    fn test_process_state_shutdown_signal() {
        let mut handles = FxHashMap::default();
        let mut closed_outputs = 0;
        let mut metrics = MessageMetrics::new();
        let output_ct = 1; // Single output

        // Process shutdown signal
        let result = process_state(
            &mut handles,
            &output_ct,
            &mut closed_outputs,
            InternalMessageState {
                message_id: crate::SHUTDOWN_MESSAGE_ID.to_string(),
                status: MessageStatus::Shutdown,
                stream_id: None,
                is_stream: false,
            },
            &mut metrics,
        );

        // Should return EndOfInput when all outputs have shut down
        assert!(result.is_err());
        assert!(matches!(result, Err(Error::EndOfInput)));
        assert_eq!(closed_outputs, 1);
    }

    #[test]
    fn test_process_state_tracks_errors() {
        let mut handles = FxHashMap::default();
        let mut closed_outputs = 0;
        let mut metrics = MessageMetrics::new();
        let message_id = "test_msg".to_string();

        // Add a message to handles
        handles.insert(
            message_id.clone(),
            State {
                instance_count: 1,
                processed_count: 0,
                processed_error_count: 0,
                output_count: 0,
                output_error_count: 0,
                closure: None,
                errors: Vec::new(),
                stream_id: None,
                stream_closed: Some(true),
                created_at: std::time::Instant::now(),
            },
        );

        // Process an error status
        let result = process_state(
            &mut handles,
            &1,
            &mut closed_outputs,
            InternalMessageState {
                message_id: message_id.clone(),
                status: MessageStatus::ProcessError("test error".to_string()),
                stream_id: None,
                is_stream: false,
            },
            &mut metrics,
        );

        assert!(result.is_ok());
        assert_eq!(metrics.total_process_errors, 1);
        // Message should be removed since stream_closed is true and counts match
        assert!(!handles.contains_key(&message_id));
    }

    #[test]
    fn test_message_status_display() {
        assert_eq!(format!("{}", MessageStatus::New), "New");
        assert_eq!(format!("{}", MessageStatus::Processed), "Processed");
        assert_eq!(
            format!("{}", MessageStatus::ProcessError("err".into())),
            "ProcessError"
        );
        assert_eq!(format!("{}", MessageStatus::Output), "Output");
        assert_eq!(
            format!("{}", MessageStatus::OutputError("err".into())),
            "OutputError"
        );
        assert_eq!(format!("{}", MessageStatus::Shutdown), "Shutdown");
        assert_eq!(
            format!("{}", MessageStatus::StreamComplete),
            "StreamComplete"
        );
    }
}
