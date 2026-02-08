#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(feature = "amqp")]
use fiddler::Runtime;
#[cfg(feature = "amqp")]
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties,
};
#[cfg(feature = "amqp")]
use testcontainers::runners::AsyncRunner;
#[cfg(feature = "amqp")]
use testcontainers_modules::rabbitmq::RabbitMq;

mod dependencies;
#[allow(unused_imports)]
use dependencies::output;

/// Test AMQP input/output: publishes to exchange, consumes from queue
#[cfg(feature = "amqp")]
#[cfg_attr(feature = "amqp", tokio::test)]
async fn fiddler_amqp_test() {
    use futures::StreamExt;

    let container = RabbitMq::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(5672).await.unwrap();
    let amqp_url = format!("amqp://guest:guest@127.0.0.1:{}/%2f", host_port);

    // Set up RabbitMQ topology
    let conn = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .unwrap();
    let channel = conn.create_channel().await.unwrap();

    // Declare input exchange and queue
    channel
        .exchange_declare(
            "input_exchange",
            lapin::ExchangeKind::Direct,
            lapin::options::ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    channel
        .queue_declare(
            "input_queue",
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    channel
        .queue_bind(
            "input_queue",
            "input_exchange",
            "input_key",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    // Declare output exchange and queue
    channel
        .exchange_declare(
            "output_exchange",
            lapin::ExchangeKind::Direct,
            lapin::options::ExchangeDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    channel
        .queue_declare(
            "output_queue",
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    channel
        .queue_bind(
            "output_queue",
            "output_exchange",
            "output_key",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    // Publish test messages to input exchange
    for i in 1..=3 {
        channel
            .basic_publish(
                "input_exchange",
                "input_key",
                BasicPublishOptions::default(),
                format!("amqp_message_{}", i).as_bytes(),
                BasicProperties::default(),
            )
            .await
            .unwrap();
    }

    // Configure fiddler pipeline
    let config = format!(
        r#"input:
  amqp:
    url: "{amqp_url}"
    queue: input_queue
    consumer_tag: fiddler_test
num_threads: 1
processors: []
output:
  amqp:
    url: "{amqp_url}"
    exchange: output_exchange
    routing_key: output_key
    persistent: true
    batch:
      size: 10"#
    );

    let mut env = Runtime::from_config(&config).await.unwrap();
    env.set_timeout(Some(tokio::time::Duration::from_secs(5)))
        .unwrap();
    env.run().await.unwrap();

    // Verify messages were forwarded to output queue
    let mut consumer = channel
        .basic_consume(
            "output_queue",
            "verify_consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    let mut received = Vec::new();
    for _ in 0..3 {
        if let Ok(Some(delivery)) =
            tokio::time::timeout(tokio::time::Duration::from_secs(2), consumer.next()).await
        {
            let delivery = delivery.unwrap();
            received.push(String::from_utf8(delivery.data.clone()).unwrap());
            delivery
                .ack(lapin::options::BasicAckOptions::default())
                .await
                .unwrap();
        }
    }

    assert_eq!(received.len(), 3, "Expected 3 messages in output_queue");
    assert!(received.contains(&"amqp_message_1".to_string()));
    assert!(received.contains(&"amqp_message_2".to_string()));
    assert!(received.contains(&"amqp_message_3".to_string()));
}
