#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]

#[cfg(all(feature = "http_server", feature = "http_client"))]
use fiddler::Runtime;
#[cfg(all(feature = "http_server", feature = "http_client"))]
use std::time::Duration;

mod dependencies;
#[allow(unused_imports)]
use dependencies::{mock, output};

#[cfg(all(feature = "http_server", feature = "http_client"))]
use std::sync::Once;

#[cfg(all(feature = "http_server", feature = "http_client"))]
static REGISTER: Once = Once::new();

#[cfg(all(feature = "http_server", feature = "http_client"))]
fn register_test_plugins() {
    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        output::register_validate().unwrap();
    });
}

/// Get an available port by binding to port 0 and releasing it.
#[cfg(all(feature = "http_server", feature = "http_client"))]
fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Wait for the HTTP server health endpoint to respond.
#[cfg(all(feature = "http_server", feature = "http_client"))]
async fn wait_for_health(port: u16) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health", port);
    for _ in 0..50 {
        if client.get(&url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("HTTP server on port {} did not become healthy", port);
}

/// Test: http output sends to http_server input with no authentication.
#[cfg(all(feature = "http_server", feature = "http_client"))]
#[tokio::test]
async fn http_output_to_http_server_no_auth() {
    register_test_plugins();
    let port = get_free_port();

    // Start receiver pipeline: http_server input -> validate output
    let receiver_config = format!(
        r#"input:
  http_server:
    address: "127.0.0.1"
    port: {port}
    path: "/ingest"
    acknowledgment: true
num_threads: 1
processors: []
output:
  validate:
    expected:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'"#
    );

    let receiver = tokio::spawn(async move {
        let mut env = Runtime::from_config(&receiver_config).await.unwrap();
        env.set_timeout(Some(Duration::from_secs(15))).unwrap();
        env.run().await
    });

    wait_for_health(port).await;

    // Start sender pipeline: mock_input -> http output
    let sender_config = format!(
        r#"input:
  mock_input:
    input:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'
num_threads: 1
processors: []
output:
  http:
    url: "http://127.0.0.1:{port}/ingest""#
    );

    let env = Runtime::from_config(&sender_config).await.unwrap();
    env.run().await.unwrap();

    // Receiver should complete via timeout after all messages are validated
    let result = receiver.await.unwrap();
    result.unwrap();
}

/// Test: http output sends to http_server input with basic authentication.
#[cfg(all(feature = "http_server", feature = "http_client"))]
#[tokio::test]
async fn http_output_to_http_server_basic_auth() {
    register_test_plugins();
    let port = get_free_port();

    // Start receiver pipeline with basic auth required
    let receiver_config = format!(
        r#"input:
  http_server:
    address: "127.0.0.1"
    port: {port}
    path: "/ingest"
    acknowledgment: true
    auth:
      type: basic
      username: admin
      password: secret123
num_threads: 1
processors: []
output:
  validate:
    expected:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'"#
    );

    let receiver = tokio::spawn(async move {
        let mut env = Runtime::from_config(&receiver_config).await.unwrap();
        env.set_timeout(Some(Duration::from_secs(15))).unwrap();
        env.run().await
    });

    wait_for_health(port).await;

    // Start sender pipeline with matching basic auth
    let sender_config = format!(
        r#"input:
  mock_input:
    input:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'
num_threads: 1
processors: []
output:
  http:
    url: "http://127.0.0.1:{port}/ingest"
    auth:
      type: basic
      username: admin
      password: secret123"#
    );

    let env = Runtime::from_config(&sender_config).await.unwrap();
    env.run().await.unwrap();

    let result = receiver.await.unwrap();
    result.unwrap();
}

/// Test: http output sends to http_server input with bearer token authentication.
#[cfg(all(feature = "http_server", feature = "http_client"))]
#[tokio::test]
async fn http_output_to_http_server_bearer_auth() {
    register_test_plugins();
    let port = get_free_port();

    // Start receiver pipeline with bearer auth required
    let receiver_config = format!(
        r#"input:
  http_server:
    address: "127.0.0.1"
    port: {port}
    path: "/ingest"
    acknowledgment: true
    auth:
      type: bearer
      token: my-secret-token
num_threads: 1
processors: []
output:
  validate:
    expected:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'"#
    );

    let receiver = tokio::spawn(async move {
        let mut env = Runtime::from_config(&receiver_config).await.unwrap();
        env.set_timeout(Some(Duration::from_secs(15))).unwrap();
        env.run().await
    });

    wait_for_health(port).await;

    // Start sender pipeline with matching bearer auth
    let sender_config = format!(
        r#"input:
  mock_input:
    input:
      - '{{"event":"login","user":"alice"}}'
      - '{{"event":"logout","user":"bob"}}'
num_threads: 1
processors: []
output:
  http:
    url: "http://127.0.0.1:{port}/ingest"
    auth:
      type: bearer
      token: my-secret-token"#
    );

    let env = Runtime::from_config(&sender_config).await.unwrap();
    env.run().await.unwrap();

    let result = receiver.await.unwrap();
    result.unwrap();
}

/// Test: http batch output (json_array format) sends to http_server input.
/// The http_server accepts JSON arrays natively, splitting each element into a
/// separate pipeline message.
#[cfg(all(feature = "http_server", feature = "http_client"))]
#[tokio::test]
async fn http_batch_output_json_array_to_http_server() {
    register_test_plugins();
    let port = get_free_port();

    // The http_server will receive a single POST containing a JSON array of 3 objects.
    // It splits the array and feeds each object individually into the pipeline.
    let receiver_config = format!(
        r#"input:
  http_server:
    address: "127.0.0.1"
    port: {port}
    path: "/ingest"
    acknowledgment: true
num_threads: 1
processors: []
output:
  validate:
    expected:
      - '{{"event":"click","id":1}}'
      - '{{"event":"scroll","id":2}}'
      - '{{"event":"submit","id":3}}'"#
    );

    let receiver = tokio::spawn(async move {
        let mut env = Runtime::from_config(&receiver_config).await.unwrap();
        env.set_timeout(Some(Duration::from_secs(15))).unwrap();
        env.run().await
    });

    wait_for_health(port).await;

    // Sender uses batch mode with json_array format.
    // All 3 messages will be collected into one batch and sent as a single
    // JSON array POST to the http_server.
    let sender_config = format!(
        r#"input:
  mock_input:
    input:
      - '{{"event":"click","id":1}}'
      - '{{"event":"scroll","id":2}}'
      - '{{"event":"submit","id":3}}'
num_threads: 1
processors: []
output:
  http:
    url: "http://127.0.0.1:{port}/ingest"
    batch:
      size: 10
      duration: "1s"
      format: "json_array""#
    );

    let env = Runtime::from_config(&sender_config).await.unwrap();
    env.run().await.unwrap();

    let result = receiver.await.unwrap();
    result.unwrap();
}

/// Test: http batch output (json_array) with bearer auth sends to http_server.
#[cfg(all(feature = "http_server", feature = "http_client"))]
#[tokio::test]
async fn http_batch_output_json_array_bearer_auth() {
    register_test_plugins();
    let port = get_free_port();

    let receiver_config = format!(
        r#"input:
  http_server:
    address: "127.0.0.1"
    port: {port}
    path: "/ingest"
    acknowledgment: true
    auth:
      type: bearer
      token: batch-secret
num_threads: 1
processors: []
output:
  validate:
    expected:
      - '{{"event":"click","id":1}}'
      - '{{"event":"scroll","id":2}}'"#
    );

    let receiver = tokio::spawn(async move {
        let mut env = Runtime::from_config(&receiver_config).await.unwrap();
        env.set_timeout(Some(Duration::from_secs(15))).unwrap();
        env.run().await
    });

    wait_for_health(port).await;

    let sender_config = format!(
        r#"input:
  mock_input:
    input:
      - '{{"event":"click","id":1}}'
      - '{{"event":"scroll","id":2}}'
num_threads: 1
processors: []
output:
  http:
    url: "http://127.0.0.1:{port}/ingest"
    auth:
      type: bearer
      token: batch-secret
    batch:
      size: 10
      duration: "1s"
      format: "json_array""#
    );

    let env = Runtime::from_config(&sender_config).await.unwrap();
    env.run().await.unwrap();

    let result = receiver.await.unwrap();
    result.unwrap();
}
