//! AWS Kinesis module for consuming from and putting records to streams.
//!
//! # Input Configuration
//!
//! ```yaml
//! input:
//!   aws_kinesis:
//!     stream_name: "my-stream"             # Required
//!     shard_id: "shardId-000000000000"     # Optional (auto-discover if not set)
//!     shard_iterator_type: "LATEST"        # Optional: LATEST, TRIM_HORIZON, AT_TIMESTAMP
//!     batch_size: 100                      # Optional (default: 100, max: 10000)
//!     region: "us-east-1"                  # Optional
//!     endpoint_url: ""                     # Optional: for LocalStack
//!     credentials:                         # Optional: use explicit credentials
//!       access_key_id: "..."
//!       secret_access_key: "..."
//! ```
//!
//! # Output Configuration
//!
//! ```yaml
//! output:
//!   aws_kinesis:
//!     stream_name: "my-stream"             # Required
//!     partition_key: "default"             # Optional (default: random UUID per record)
//!     region: "us-east-1"                  # Optional
//!     endpoint_url: ""                     # Optional: for LocalStack
//!     credentials:                         # Optional: use explicit credentials
//!       access_key_id: "..."
//!       secret_access_key: "..."
//!     batch:
//!       size: 500                          # Optional (max: 500 for Kinesis)
//!       duration: "5s"
//! ```

use crate::config::{register_plugin, ConfigSpec, ExecutionType, ItemType};
use crate::{BatchingPolicy, CallbackChan, Closer, Error, Input, Message, MessageBatch, OutputBatch};
use async_trait::async_trait;
use aws_sdk_kinesis::primitives::Blob;
use aws_sdk_kinesis::types::{PutRecordsRequestEntry, ShardIteratorType};
use aws_sdk_kinesis::Client;
use fiddler_macros::fiddler_registration_func;
use flume::{bounded, Receiver, Sender};
use serde::Deserialize;
use serde_yaml::Value;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{debug, error, warn};
use uuid::Uuid;

const DEFAULT_BATCH_SIZE: i32 = 100;
const MAX_KINESIS_BATCH: usize = 500;
const CHANNEL_BUFFER: usize = 100;

/// Kinesis input configuration.
#[derive(Deserialize, Clone)]
pub struct KinesisInputConfig {
    pub stream_name: String,
    pub shard_id: Option<String>,
    #[serde(default = "default_iterator_type")]
    pub shard_iterator_type: String,
    #[serde(default = "default_batch_size")]
    pub batch_size: i32,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub credentials: Option<super::Credentials>,
}

/// Kinesis output configuration.
#[derive(Deserialize, Clone)]
pub struct KinesisOutputConfig {
    pub stream_name: String,
    pub partition_key: Option<String>,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub credentials: Option<super::Credentials>,
    #[serde(default)]
    pub batch: Option<BatchingPolicy>,
}

fn default_iterator_type() -> String {
    "LATEST".to_string()
}

fn default_batch_size() -> i32 {
    DEFAULT_BATCH_SIZE
}

fn parse_iterator_type(s: &str) -> ShardIteratorType {
    match s.to_uppercase().as_str() {
        "TRIM_HORIZON" => ShardIteratorType::TrimHorizon,
        "AT_TIMESTAMP" => ShardIteratorType::AtTimestamp,
        "AT_SEQUENCE_NUMBER" => ShardIteratorType::AtSequenceNumber,
        "AFTER_SEQUENCE_NUMBER" => ShardIteratorType::AfterSequenceNumber,
        _ => ShardIteratorType::Latest,
    }
}

/// Build a Kinesis client from configuration.
async fn build_client(
    region: Option<String>,
    endpoint_url: Option<String>,
    credentials: Option<super::Credentials>,
) -> Result<Client, Error> {
    let mut aws_config = aws_config::from_env();
    if let Some(ref region) = region {
        aws_config = aws_config.region(aws_sdk_kinesis::config::Region::new(region.clone()));
    }
    let sdk_config = aws_config.load().await;

    let mut client_config = aws_sdk_kinesis::config::Builder::from(&sdk_config);

    if let Some(creds) = credentials {
        let provider = aws_sdk_kinesis::config::Credentials::new(
            creds.access_key_id,
            creds.secret_access_key,
            creds.session_token,
            None,
            "fiddler",
        );
        client_config = client_config.credentials_provider(provider);
    }

    if let Some(ref endpoint) = endpoint_url {
        if !endpoint.is_empty() {
            client_config = client_config.endpoint_url(endpoint);
        }
    }

    Ok(Client::from_conf(client_config.build()))
}

// ============================================================================
// Input Implementation
// ============================================================================

async fn kinesis_reader_task(
    config: KinesisInputConfig,
    sender: Sender<Result<Message, Error>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let client = match build_client(
        config.region.clone(),
        config.endpoint_url.clone(),
        config.credentials.clone(),
    ).await {
        Ok(c) => c,
        Err(e) => {
            let _ = sender.send_async(Err(e)).await;
            return;
        }
    };

    // Get shard ID (either from config or discover first shard)
    let shard_id = match config.shard_id {
        Some(ref id) => id.clone(),
        None => {
            match client.list_shards().stream_name(&config.stream_name).send().await {
                Ok(resp) => {
                    if let Some(shards) = resp.shards {
                        if let Some(shard) = shards.first() {
                            shard.shard_id().to_string()
                        } else {
                            let _ = sender.send_async(Err(Error::ExecutionError("No shards found".into()))).await;
                            return;
                        }
                    } else {
                        let _ = sender.send_async(Err(Error::ExecutionError("No shards found".into()))).await;
                        return;
                    }
                }
                Err(e) => {
                    let _ = sender.send_async(Err(Error::ExecutionError(format!("List shards failed: {}", e)))).await;
                    return;
                }
            }
        }
    };

    // Get initial shard iterator
    let iterator_type = parse_iterator_type(&config.shard_iterator_type);
    let mut shard_iterator = match client
        .get_shard_iterator()
        .stream_name(&config.stream_name)
        .shard_id(&shard_id)
        .shard_iterator_type(iterator_type)
        .send()
        .await
    {
        Ok(resp) => resp.shard_iterator,
        Err(e) => {
            let _ = sender.send_async(Err(Error::ExecutionError(format!("Get iterator failed: {}", e)))).await;
            return;
        }
    };

    debug!(stream = %config.stream_name, shard = %shard_id, "Kinesis consumer started");

    loop {
        if shutdown_rx.try_recv().is_ok() {
            let _ = sender.send_async(Err(Error::EndOfInput)).await;
            return;
        }

        let iterator = match &shard_iterator {
            Some(it) => it.clone(),
            None => {
                warn!("Shard iterator expired, attempting to refresh");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        match client
            .get_records()
            .shard_iterator(&iterator)
            .limit(config.batch_size)
            .send()
            .await
        {
            Ok(resp) => {
                shard_iterator = resp.next_shard_iterator;

                for record in resp.records {
                    let msg = Message {
                        bytes: record.data.into_inner(),
                        ..Default::default()
                    };
                    if sender.send_async(Ok(msg)).await.is_err() {
                        return;
                    }
                }

                // If no records, brief sleep to avoid hammering
                if resp.millis_behind_latest.unwrap_or(0) == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
            Err(e) => {
                error!(error = %e, "Kinesis get_records failed");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

pub struct KinesisInput {
    receiver: Receiver<Result<Message, Error>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl KinesisInput {
    pub fn new(config: KinesisInputConfig) -> Result<Self, Error> {
        let (sender, receiver) = bounded(CHANNEL_BUFFER);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(kinesis_reader_task(config, sender, shutdown_rx));

        Ok(Self {
            receiver,
            shutdown_tx: Some(shutdown_tx),
        })
    }
}

#[async_trait]
impl Input for KinesisInput {
    async fn read(&mut self) -> Result<(Message, Option<CallbackChan>), Error> {
        match self.receiver.recv_async().await {
            Ok(Ok(msg)) => Ok((msg, None)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::EndOfInput),
        }
    }
}

#[async_trait]
impl Closer for KinesisInput {
    async fn close(&mut self) -> Result<(), Error> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        debug!("Kinesis input closed");
        Ok(())
    }
}

// ============================================================================
// Output Implementation
// ============================================================================

pub struct KinesisOutput {
    client: Client,
    stream_name: String,
    partition_key: Option<String>,
    batch_size: usize,
    interval: Duration,
}

impl KinesisOutput {
    pub async fn new(config: KinesisOutputConfig) -> Result<Self, Error> {
        let client = build_client(
            config.region.clone(),
            config.endpoint_url.clone(),
            config.credentials.clone(),
        ).await?;

        // Enforce Kinesis limit of 500 records per PutRecords call
        let batch_size = config
            .batch
            .as_ref()
            .map_or(MAX_KINESIS_BATCH, |b| b.effective_size().min(MAX_KINESIS_BATCH));

        let interval = config
            .batch
            .as_ref()
            .map_or(Duration::from_secs(5), |b| b.effective_duration());

        debug!(stream = %config.stream_name, "Kinesis output initialized");

        Ok(Self {
            client,
            stream_name: config.stream_name,
            partition_key: config.partition_key,
            batch_size,
            interval,
        })
    }
}

#[async_trait]
impl OutputBatch for KinesisOutput {
    async fn write_batch(&mut self, messages: MessageBatch) -> Result<(), Error> {
        if messages.is_empty() {
            return Ok(());
        }

        let records: Vec<PutRecordsRequestEntry> = messages
            .iter()
            .map(|msg| {
                let pk = self
                    .partition_key
                    .clone()
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                PutRecordsRequestEntry::builder()
                    .data(Blob::new(msg.bytes.clone()))
                    .partition_key(pk)
                    .build()
                    .expect("valid entry")
            })
            .collect();

        let result = self
            .client
            .put_records()
            .stream_name(&self.stream_name)
            .set_records(Some(records))
            .send()
            .await;

        match result {
            Ok(resp) => {
                let failed = resp.failed_record_count.unwrap_or(0);
                if failed > 0 {
                    warn!(failed_count = failed, "Some Kinesis records failed");
                }
                debug!(count = messages.len(), failed = failed, "Put records to Kinesis");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Kinesis put_records failed");
                Err(Error::OutputError(format!("Kinesis put failed: {}", e)))
            }
        }
    }

    async fn batch_size(&self) -> usize {
        self.batch_size
    }

    async fn interval(&self) -> Duration {
        self.interval
    }
}

#[async_trait]
impl Closer for KinesisOutput {
    async fn close(&mut self) -> Result<(), Error> {
        debug!("Kinesis output closed");
        Ok(())
    }
}

// ============================================================================
// Plugin Registration
// ============================================================================

#[fiddler_registration_func]
fn create_kinesis_input(conf: Value) -> Result<ExecutionType, Error> {
    let config: KinesisInputConfig = serde_yaml::from_value(conf)?;

    if config.stream_name.is_empty() {
        return Err(Error::ConfigFailedValidation("stream_name is required".into()));
    }

    Ok(ExecutionType::Input(Box::new(KinesisInput::new(config)?)))
}

#[fiddler_registration_func]
fn create_kinesis_output(conf: Value) -> Result<ExecutionType, Error> {
    let config: KinesisOutputConfig = serde_yaml::from_value(conf)?;

    if config.stream_name.is_empty() {
        return Err(Error::ConfigFailedValidation("stream_name is required".into()));
    }

    Ok(ExecutionType::OutputBatch(Box::new(
        KinesisOutput::new(config).await?,
    )))
}

pub(super) fn register_kinesis() -> Result<(), Error> {
    let input_config = r#"type: object
required:
  - stream_name
properties:
  stream_name:
    type: string
    description: "Kinesis stream name"
  shard_id:
    type: string
    description: "Specific shard ID (auto-discover if not set)"
  shard_iterator_type:
    type: string
    enum: ["LATEST", "TRIM_HORIZON", "AT_TIMESTAMP"]
    default: "LATEST"
    description: "Iterator type"
  batch_size:
    type: integer
    default: 100
    description: "Records per GetRecords call"
  region:
    type: string
    description: "AWS region"
  endpoint_url:
    type: string
    description: "Custom endpoint URL (for LocalStack)"
  credentials:
    type: object
    properties:
      access_key_id:
        type: string
      secret_access_key:
        type: string
      session_token:
        type: string
    required:
      - access_key_id
      - secret_access_key
"#;

    let output_config = r#"type: object
required:
  - stream_name
properties:
  stream_name:
    type: string
    description: "Kinesis stream name"
  partition_key:
    type: string
    description: "Partition key (random UUID if not set)"
  region:
    type: string
    description: "AWS region"
  endpoint_url:
    type: string
    description: "Custom endpoint URL (for LocalStack)"
  credentials:
    type: object
    properties:
      access_key_id:
        type: string
      secret_access_key:
        type: string
      session_token:
        type: string
    required:
      - access_key_id
      - secret_access_key
  batch:
    type: object
    properties:
      size:
        type: integer
        description: "Batch size (max: 500)"
      duration:
        type: string
        description: "Flush interval"
"#;

    let input_spec = ConfigSpec::from_schema(input_config)?;
    let output_spec = ConfigSpec::from_schema(output_config)?;

    register_plugin(
        "aws_kinesis".into(),
        ItemType::Input,
        input_spec,
        create_kinesis_input,
    )?;

    register_plugin(
        "aws_kinesis".into(),
        ItemType::OutputBatch,
        output_spec,
        create_kinesis_output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_config_deserialization() {
        let yaml = r#"
stream_name: "my-stream"
shard_id: "shardId-000000000000"
shard_iterator_type: "TRIM_HORIZON"
batch_size: 200
region: "us-west-2"
"#;
        let config: KinesisInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.stream_name, "my-stream");
        assert_eq!(config.shard_id, Some("shardId-000000000000".to_string()));
        assert_eq!(config.batch_size, 200);
    }

    #[test]
    fn test_input_config_defaults() {
        let yaml = r#"stream_name: "test-stream""#;
        let config: KinesisInputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.shard_iterator_type, "LATEST");
        assert_eq!(config.batch_size, 100);
        assert!(config.shard_id.is_none());
    }

    #[test]
    fn test_output_config_deserialization() {
        let yaml = r#"
stream_name: "my-stream"
partition_key: "customer_id"
region: "eu-west-1"
"#;
        let config: KinesisOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.stream_name, "my-stream");
        assert_eq!(config.partition_key, Some("customer_id".to_string()));
        assert_eq!(config.region, Some("eu-west-1".to_string()));
    }

    #[test]
    fn test_output_config_defaults() {
        let yaml = r#"stream_name: "test-stream""#;
        let config: KinesisOutputConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.partition_key.is_none());
        assert!(config.region.is_none());
        assert!(config.batch.is_none());
    }

    #[test]
    fn test_register() {
        let result = register_kinesis();
        assert!(result.is_ok() || matches!(result, Err(Error::DuplicateRegisteredName(_))));
    }
}
