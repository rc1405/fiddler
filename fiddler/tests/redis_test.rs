#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(feature = "redis")]
use fiddler::Runtime;
#[cfg(feature = "redis")]
use redis::AsyncCommands;
#[cfg(feature = "redis")]
use testcontainers::runners::AsyncRunner;
#[cfg(feature = "redis")]
use testcontainers_modules::redis::Redis;

mod dependencies;
#[allow(unused_imports)]
use dependencies::output;

/// Test Redis list mode: input reads from list, output writes to another list
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_list_input_test() {
    let container = Redis::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    // Wait for Redis to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Create Redis client to seed data
    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    // Seed input data
    let _: () = conn.rpush("input_queue", "message1").await.unwrap();
    let _: () = conn.rpush("input_queue", "message2").await.unwrap();
    let _: () = conn.rpush("input_queue", "message3").await.unwrap();

    // Verify data was seeded
    let len: usize = conn.llen("input_queue").await.unwrap();
    assert_eq!(len, 3, "Expected 3 messages seeded in input_queue");

    // Quick test that BLPOP works directly with integer timeout
    let result: Option<(String, String)> = redis::cmd("BLPOP")
        .arg(&["input_queue"])
        .arg(1_u64)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(result.is_some(), "BLPOP should return a message");

    // Re-seed the message we just popped
    let _: () = conn.rpush("input_queue", "message1").await.unwrap();

    // Use drop output - we'll verify by checking input_queue is drained
    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: list
    keys:
      - input_queue
    list_command: blpop
    timeout: 2
num_threads: 1
processors: []
output:
  drop: {{}}"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
        .unwrap();
    env.run().await.unwrap();

    // Verify input_queue was drained (messages were read)
    let len: usize = conn.llen("input_queue").await.unwrap();
    assert_eq!(len, 0, "Expected input_queue to be drained after processing");
}

/// Test Redis pubsub: input subscribes, output writes to list
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_pubsub_input_test() {
    let container = Redis::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    // Wait for Redis to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let _ = output::register_validate();

    // Configure fiddler pipeline: subscribe to channel, validate output
    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: pubsub
    channels:
      - test_channel
num_threads: 1
processors: []
output:
  validate:
    expected:
      - "pubsub_msg_1"
      - "pubsub_msg_2"
      - "pubsub_msg_3""#
    );

    // Spawn the pipeline in a background task
    let env_handle = {
        let config = config.clone();
        tokio::spawn(async move {
            let mut env = Runtime::from_config(&config).await.unwrap();
            env.set_timeout(Some(tokio::time::Duration::from_secs(10)))
                .unwrap();
            env.run().await
        })
    };

    // Give the subscriber time to connect
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Publish messages from a separate connection
    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    for i in 1..=3 {
        let _: () = conn.publish("test_channel", format!("pubsub_msg_{}", i)).await.unwrap();
        // Small delay between publishes
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Wait for pipeline to complete
    let result = env_handle.await.unwrap();
    result.unwrap();
}
