#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(feature = "aws")]
use aws_sdk_kinesis as kinesis;
#[cfg(feature = "aws")]
use aws_sdk_kinesis::config::BehaviorVersion;
#[cfg(feature = "aws")]
use aws_sdk_kinesis::config::Region;
#[cfg(feature = "aws")]
use fiddler::Runtime;
#[cfg(feature = "aws")]
use testcontainers::{runners::AsyncRunner, ImageExt};
#[cfg(feature = "aws")]
use testcontainers_modules::localstack::LocalStack;

mod dependencies;
#[allow(unused_imports)]
use dependencies::output;

/// Test Kinesis input/output: reads from stream, writes to another stream
#[cfg(feature = "aws")]
#[cfg_attr(feature = "aws", tokio::test)]
async fn fiddler_kinesis_test() {
    let request = LocalStack::default().with_env_var("SERVICES", "kinesis");
    let container = request.start().await.unwrap();

    let host_ip = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(4566).await.unwrap();
    let endpoint_url = format!("http://{}:{}", host_ip, host_port);
    let creds = kinesis::config::Credentials::new("fake", "fake", None, None, "test");

    std::env::set_var("AWS_ACCESS_KEY_ID", "fake");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "fake");
    std::env::set_var("AWS_DEFAULT_REGION", "us-east-1");

    let config = kinesis::config::Builder::default()
        .behavior_version(BehaviorVersion::v2025_01_17())
        .region(Region::new("us-east-1"))
        .credentials_provider(creds)
        .endpoint_url(&endpoint_url)
        .build();

    let client = kinesis::Client::from_conf(config);

    // Create input stream
    client
        .create_stream()
        .stream_name("input_stream")
        .shard_count(1)
        .send()
        .await
        .unwrap();

    // Create output stream
    client
        .create_stream()
        .stream_name("output_stream")
        .shard_count(1)
        .send()
        .await
        .unwrap();

    // Wait for streams to become active
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Put records to input stream
    for i in 1..=3 {
        client
            .put_record()
            .stream_name("input_stream")
            .partition_key("test_partition")
            .data(kinesis::primitives::Blob::new(format!("kinesis_message_{}", i)))
            .send()
            .await
            .unwrap();
    }

    // Configure fiddler pipeline
    let fiddler_config = format!(
        r#"input:
  aws_kinesis:
    stream_name: input_stream
    shard_iterator_type: TRIM_HORIZON
    batch_size: 100
    region: us-east-1
    endpoint_url: {endpoint_url}
num_threads: 1
processors: []
output:
  aws_kinesis:
    stream_name: output_stream
    partition_key: fiddler_partition
    region: us-east-1
    endpoint_url: {endpoint_url}
    batch:
      size: 10"#
    );

    let mut env = Runtime::from_config(&fiddler_config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(10)))
        .unwrap();
    env.run().await.unwrap();

    // Get shard iterator for output stream to verify messages
    let shards = client
        .list_shards()
        .stream_name("output_stream")
        .send()
        .await
        .unwrap();

    let shard_id = shards
        .shards
        .as_ref()
        .and_then(|s| s.first())
        .map(|s| s.shard_id())
        .unwrap();

    let iterator = client
        .get_shard_iterator()
        .stream_name("output_stream")
        .shard_id(shard_id)
        .shard_iterator_type(kinesis::types::ShardIteratorType::TrimHorizon)
        .send()
        .await
        .unwrap();

    let records = client
        .get_records()
        .shard_iterator(iterator.shard_iterator.unwrap())
        .send()
        .await
        .unwrap();

    let messages: Vec<String> = records
        .records
        .iter()
        .map(|r| String::from_utf8(r.data.as_ref().to_vec()).unwrap())
        .collect();

    assert_eq!(messages.len(), 3, "Expected 3 messages in output_stream");
    assert!(messages.contains(&"kinesis_message_1".to_string()));
    assert!(messages.contains(&"kinesis_message_2".to_string()));
    assert!(messages.contains(&"kinesis_message_3".to_string()));
}
