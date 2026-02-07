#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(feature = "mqtt")]
use fiddler::Runtime;
#[cfg(feature = "mqtt")]
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
#[cfg(feature = "mqtt")]
use testcontainers::runners::AsyncRunner;
#[cfg(feature = "mqtt")]
use testcontainers_modules::mosquitto::Mosquitto;

mod dependencies;
#[allow(unused_imports)]
use dependencies::output;

/// Test MQTT input/output: subscribes to input topic, publishes to output topic
#[cfg(feature = "mqtt")]
#[cfg_attr(feature = "mqtt", tokio::test)]
async fn fiddler_mqtt_test() {
    let container = Mosquitto::default().start().await.unwrap();
    let host_port = container.get_host_port_ipv4(1883).await.unwrap();

    // Wait for Mosquitto to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let _ = output::register_validate();

    // Configure fiddler pipeline: subscribe to input, validate output
    let config = format!(
        r#"input:
  mqtt:
    broker: "127.0.0.1:{host_port}"
    client_id: fiddler_input
    topics:
      - input/topic
    qos: 1
num_threads: 1
processors: []
output:
  validate:
    expected:
      - "mqtt_msg_1"
      - "mqtt_msg_2"
      - "mqtt_msg_3""#
    );

    // Spawn the pipeline in a background task
    let env_handle = {
        let config = config.clone();
        tokio::spawn(async move {
            let mut env = Runtime::from_config(&config).await.unwrap();
            env.set_timeout(Some(tokio::time::Duration::from_secs(15)))
                .unwrap();
            env.run().await
        })
    };

    // Give the fiddler subscriber time to connect
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Publish test messages to input topic
    let mut pub_opts = MqttOptions::new("pub_client", "127.0.0.1", host_port);
    pub_opts.set_keep_alive(std::time::Duration::from_secs(30));
    let (pub_client, mut pub_eventloop) = AsyncClient::new(pub_opts, 10);

    // Start publisher eventloop in background
    tokio::spawn(async move {
        loop {
            if pub_eventloop.poll().await.is_err() {
                break;
            }
        }
    });

    // Small delay for publisher to connect
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Publish messages
    for i in 1..=3 {
        pub_client
            .publish(
                "input/topic",
                QoS::AtLeastOnce,
                false,
                format!("mqtt_msg_{}", i),
            )
            .await
            .unwrap();
        // Small delay between publishes
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Wait for pipeline to complete
    let result = env_handle.await.unwrap();
    result.unwrap();
}
