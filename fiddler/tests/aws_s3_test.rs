#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]
#[cfg(feature = "aws")]
use aws_sdk_s3 as s3;
#[cfg(feature = "aws")]
use aws_sdk_sqs as sqs;
#[cfg(feature = "aws")]
use aws_sdk_sqs::config::BehaviorVersion;
#[cfg(feature = "aws")]
use aws_sdk_sqs::config::Region;
#[cfg(feature = "aws")]
use aws_smithy_types::byte_stream::ByteStream;
#[cfg(feature = "aws")]
use fiddler::Runtime;
#[allow(unused_imports)]
use std::path::MAIN_SEPARATOR_STR;
#[cfg(feature = "aws")]
use testcontainers::{runners::AsyncRunner, ImageExt};
#[cfg(feature = "aws")]
use testcontainers_modules::localstack::LocalStack;
#[cfg(feature = "aws")]
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

mod dependencies;
#[allow(unused_imports)]
use dependencies::output;

#[cfg(feature = "aws")]
#[cfg_attr(feature = "aws", tokio::test)]
async fn fiddler_aws_s3_test() {
    use aws_sdk_s3::primitives::ByteStream;
    use tokio::time::Sleep;

    let request = LocalStack::default().with_env_var("SERVICES", "sqs,s3");
    let container = request.start().await.unwrap();

    let host_ip = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(4566).await.unwrap();
    let endpoint_url = format!("http://{host_ip}:{host_port}");
    let creds = sqs::config::Credentials::new("fake", "fake", None, None, "test");

    let sqs_config = sqs::config::Builder::default()
        .behavior_version(BehaviorVersion::v2025_01_17())
        .region(Region::new("us-east-1"))
        .credentials_provider(creds.clone())
        .endpoint_url(&endpoint_url)
        .build();

    let sqs_client = sqs::Client::from_conf(sqs_config);

    let s3_config = s3::config::Builder::default()
        .behavior_version(BehaviorVersion::v2025_01_17())
        .region(Region::new("us-east-1"))
        .credentials_provider(creds)
        .endpoint_url(&endpoint_url)
        .force_path_style(true)
        .build();

    let s3_client = s3::Client::from_conf(s3_config);

    let _bucket = s3_client
        .create_bucket()
        .bucket("testing-bucket")
        .send()
        .await
        .unwrap();

    let in_queue = sqs_client
        .create_queue()
        .queue_name("in_queue")
        .send()
        .await
        .unwrap();

    let in_queue_url = in_queue.queue_url().unwrap();

    let queue = sqs_client
        .get_queue_attributes()
        .queue_url(in_queue_url)
        .attribute_names(sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap();

    let attributes = queue.attributes().unwrap();

    let arn = attributes
        .get(&sqs::types::QueueAttributeName::QueueArn)
        .unwrap();

    let queue_config = s3::types::QueueConfiguration::builder()
        .queue_arn(arn)
        .events(s3::types::Event::S3ObjectCreated)
        .build()
        .unwrap();

    let notification_config = s3::types::NotificationConfiguration::builder()
        .queue_configurations(queue_config)
        .build();

    let _notification = s3_client
        .put_bucket_notification_configuration()
        .bucket("testing-bucket")
        .notification_configuration(notification_config)
        .send()
        .await
        .unwrap();

    let data = ByteStream::read_from()
        .path(format!(
            "tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt"
        ))
        .build()
        .await
        .unwrap();

    let _obj1 = s3_client
        .put_object()
        .bucket("testing-bucket")
        .key("this/is/a/test/2025/23/12/input.txt")
        .body(data)
        .send()
        .await
        .unwrap();

    let _ = output::register_validate();

    let config2 = format!(
        "input:
  aws_s3:
    bucket: testing-bucket
    read_lines: true
    queue: 
      queue_url: {in_queue_url}
      endpoint_url: {endpoint_url}
      region: us-east-1
      credentials:
        access_key_id: fake
        secret_access_key: fake
    endpoint_url: {endpoint_url}
    region: us-east-1
    force_path_style_urls: true
    delete_after_read: false
    credentials: 
      access_key_id: fake
      secret_access_key: fake
num_threads: 1
processors: []
output:
  validate: 
    expected: 
      - 'Hello World'
      - 'This is the end'",
    );

    let mut env2 = Runtime::from_config(&config2).await.unwrap();
    env2.set_timeout(Some(tokio::time::Duration::from_secs(5)))
        .unwrap();
    env2.run().await.unwrap();

    let config1 = format!(
        "input:
  aws_s3:
    bucket: testing-bucket
    read_lines: true
    endpoint_url: {endpoint_url}
    region: us-east-1
    force_path_style_urls: true
    delete_after_read: true
    credentials: 
      access_key_id: fake
      secret_access_key: fake
num_threads: 1
processors: []
output:
  validate: 
    expected: 
      - 'Hello World'
      - 'This is the end'",
    );

    let mut env1 = Runtime::from_config(&config1).await.unwrap();
    env1.run().await.unwrap();

    let results = s3_client
        .list_objects()
        .bucket("testing-bucket")
        .send()
        .await
        .unwrap();

    assert!(results.contents().len() == 0, "bucket still has objects");
}
