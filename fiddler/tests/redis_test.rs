#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(feature = "redis")]
use fiddler::Runtime;
#[cfg(feature = "redis")]
use redis::AsyncCommands;
#[cfg(feature = "redis")]
use redis::FromRedisValue;
#[cfg(feature = "redis")]
use testcontainers::runners::AsyncRunner;
#[cfg(feature = "redis")]
use testcontainers::GenericImage;
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
    assert_eq!(
        len, 0,
        "Expected input_queue to be drained after processing"
    );
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
        let _: () = conn
            .publish("test_channel", format!("pubsub_msg_{}", i))
            .await
            .unwrap();
        // Small delay between publishes
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Wait for pipeline to complete
    let result = env_handle.await.unwrap();
    result.unwrap();
}

/// Test Redis stream mode: output writes via XADD, input reads via XREADGROUP
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_stream_roundtrip_test() {
    let container = Redis::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    for i in 1..=3 {
        let _: String = redis::cmd("XADD")
            .arg("test_stream")
            .arg("*")
            .arg("data")
            .arg(format!("stream_msg_{}", i))
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    let len: usize = redis::cmd("XLEN")
        .arg("test_stream")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(len, 3, "Expected 3 messages in test_stream");

    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: stream
    streams:
      - test_stream
    consumer_group: test_group
    consumer_name: test_consumer
    block_ms: 2000
    auto_claim:
      enabled: false
    create_group: true
num_threads: 1
processors: []
output:
  drop: {{}}"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
        .unwrap();
    env.run().await.unwrap();

    let pending: redis::Value = redis::cmd("XPENDING")
        .arg("test_stream")
        .arg("test_group")
        .query_async(&mut conn)
        .await
        .unwrap();

    match pending {
        redis::Value::Array(ref arr) if !arr.is_empty() => {
            let count: i64 = i64::from_redis_value(&arr[0]).unwrap();
            assert_eq!(count, 0, "Expected no pending messages after processing");
        }
        _ => {}
    }
}

/// Test Redis stream output with MAXLEN trimming
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_stream_maxlen_trim_test() {
    let container = Redis::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    for i in 1..=200 {
        let _: () = conn
            .rpush("input_list", format!("msg_{}", i))
            .await
            .unwrap();
    }

    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: list
    keys:
      - input_list
    list_command: blpop
    timeout: 2
num_threads: 1
processors: []
output:
  redis:
    url: "{redis_url}"
    mode: stream
    stream: output_stream
    max_len: 50"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
        .unwrap();
    env.run().await.unwrap();

    let len: usize = redis::cmd("XLEN")
        .arg("output_stream")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        len <= 150,
        "Expected stream length ~50 with approximate trimming, got {}",
        len
    );
}

/// Test Redis stream pending message recovery on restart
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_stream_pending_recovery_test() {
    let container = Redis::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    let _: () = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg("recovery_stream")
        .arg("recovery_group")
        .arg("0")
        .arg("MKSTREAM")
        .query_async(&mut conn)
        .await
        .unwrap();

    for i in 1..=3 {
        let _: String = redis::cmd("XADD")
            .arg("recovery_stream")
            .arg("*")
            .arg("data")
            .arg(format!("recover_msg_{}", i))
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    let _: redis::Value = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("recovery_group")
        .arg("crash_consumer")
        .arg("COUNT")
        .arg(3)
        .arg("STREAMS")
        .arg("recovery_stream")
        .arg(">")
        .query_async(&mut conn)
        .await
        .unwrap();

    let pending: redis::Value = redis::cmd("XPENDING")
        .arg("recovery_stream")
        .arg("recovery_group")
        .query_async(&mut conn)
        .await
        .unwrap();
    match &pending {
        redis::Value::Array(ref arr) if !arr.is_empty() => {
            let count: i64 = i64::from_redis_value(&arr[0]).unwrap();
            assert_eq!(count, 3, "Expected 3 pending messages");
        }
        _ => panic!("Expected pending messages"),
    }

    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: stream
    streams:
      - recovery_stream
    consumer_group: recovery_group
    consumer_name: crash_consumer
    block_ms: 2000
    auto_claim:
      enabled: false
    create_group: false
num_threads: 1
processors: []
output:
  drop: {{}}"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
        .unwrap();
    env.run().await.unwrap();

    let pending: redis::Value = redis::cmd("XPENDING")
        .arg("recovery_stream")
        .arg("recovery_group")
        .query_async(&mut conn)
        .await
        .unwrap();
    match &pending {
        redis::Value::Array(ref arr) if !arr.is_empty() => {
            let count: i64 = i64::from_redis_value(&arr[0]).unwrap();
            assert_eq!(count, 0, "Expected no pending messages after recovery");
        }
        _ => {}
    }
}

/// Test Redis stream auto-claim reclaims idle messages from dead consumer.
/// Requires Redis 6.2+ for XAUTOCLAIM support.
#[cfg(feature = "redis")]
#[cfg_attr(feature = "redis", tokio::test)]
async fn fiddler_redis_stream_auto_claim_test() {
    let redis_image = GenericImage::new("redis", "7").with_wait_for(
        testcontainers::core::WaitFor::message_on_stdout("Ready to accept connections"),
    );
    let container = redis_image.start().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();
    let redis_url = format!("redis://127.0.0.1:{}", host_port);

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = redis::Client::open(redis_url.as_str()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();

    let _: () = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg("claim_stream")
        .arg("claim_group")
        .arg("0")
        .arg("MKSTREAM")
        .query_async(&mut conn)
        .await
        .unwrap();

    for i in 1..=3 {
        let _: String = redis::cmd("XADD")
            .arg("claim_stream")
            .arg("*")
            .arg("data")
            .arg(format!("claim_msg_{}", i))
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    let _: redis::Value = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("claim_group")
        .arg("dead_consumer")
        .arg("COUNT")
        .arg(3)
        .arg("STREAMS")
        .arg("claim_stream")
        .arg(">")
        .query_async(&mut conn)
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let config = format!(
        r#"input:
  redis:
    url: "{redis_url}"
    mode: stream
    streams:
      - claim_stream
    consumer_group: claim_group
    consumer_name: live_consumer
    block_ms: 2000
    auto_claim:
      enabled: true
      idle_ms: 100
      interval_ms: 500
      batch_size: 10
    create_group: false
num_threads: 1
processors: []
output:
  drop: {{}}"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
        .unwrap();
    env.run().await.unwrap();

    // Give spawned XACK tasks time to complete after pipeline shutdown
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let pending: redis::Value = redis::cmd("XPENDING")
        .arg("claim_stream")
        .arg("claim_group")
        .query_async(&mut conn)
        .await
        .unwrap();
    match &pending {
        redis::Value::Array(ref arr) if !arr.is_empty() => {
            let count: i64 = i64::from_redis_value(&arr[0]).unwrap();
            assert_eq!(count, 0, "Expected no pending messages after auto-claim");
        }
        _ => {}
    }
}
