#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]
#[cfg(feature = "aws")]
use aws_sdk_sqs as sqs;
#[cfg(feature = "aws")]
use aws_sdk_sqs::config::BehaviorVersion;
#[cfg(feature = "aws")]
use aws_sdk_sqs::config::Region;
#[cfg(feature = "aws")]
use fiddler::Runtime;
#[cfg(feature = "aws")]
use testcontainers::{runners::AsyncRunner, ImageExt};
#[cfg(feature = "aws")]
use testcontainers_modules::localstack::LocalStack;

#[cfg(feature = "aws")]
#[cfg_attr(feature = "aws", tokio::test)]
async fn fiddler_aws_sqs_test() {
    let request = LocalStack::default().with_env_var("SERVICES", "sqs");
    let container = request.start().await.unwrap();

    let host_ip = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(4566).await.unwrap();
    let endpoint_url = format!("http://{host_ip}:{host_port}");
    let creds = sqs::config::Credentials::new("fake", "fake", None, None, "test");
    std::env::set_var("AWS_ACCESS_KEY_ID", "fake");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "fake");
    std::env::set_var("AWS_DEFAULT_REGION", "us-east-1");

    let config = sqs::config::Builder::default()
        .behavior_version(BehaviorVersion::v2025_01_17())
        .region(Region::new("us-east-1"))
        .credentials_provider(creds)
        .endpoint_url(&endpoint_url)
        .build();

    let client = sqs::Client::from_conf(config);

    let in_queue = client
        .create_queue()
        .queue_name("in_queue")
        .send()
        .await
        .unwrap();

    let in_queue_url = in_queue.queue_url().unwrap();

    let _ = client
        .send_message()
        .queue_url(in_queue_url)
        .message_body("Testing Message to send to SQS")
        .send()
        .await
        .unwrap();

    let out_queue = client
        .create_queue()
        .queue_name("out_queue")
        .send()
        .await
        .unwrap();

    let out_queue_url = out_queue.queue_url().unwrap();

    let config = format!(
        "input:
  aws_sqs:
    queue_url: {}
    endpoint_url: {}
    region: us-east-1
    credentials: 
      access_key_id: fake
      secret_access_key: fake
num_threads: 1
processors: []
output:
  aws_sqs:
    queue_url: {}
    endpoint_url: {}
    region: us-east-1
    credentials: 
      access_key_id: fake
      secret_access_key: fake",
        in_queue_url, endpoint_url, out_queue_url, endpoint_url
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(5)))
        .unwrap();
    env.run().await.unwrap();

    let result = client
        .receive_message()
        .queue_url(out_queue_url)
        .send()
        .await
        .unwrap();

    assert_eq!(result.messages.is_some(), true);
    let messages = result.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].body().unwrap(),
        "Testing Message to send to SQS"
    );
}
