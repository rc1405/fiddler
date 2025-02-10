#[cfg(feature = "elasticsearch")]
use testcontainers_modules::{
    elastic_search, testcontainers::clients::Cli, testcontainers::RunnableImage,
};

#[cfg(feature = "elasticsearch")]
use elasticsearch::{http::transport::Transport, Elasticsearch, SearchParts};

#[allow(unused_imports)]
use fiddler::Runtime;
#[allow(unused_imports)]
use serde_json::{json, Value};
#[allow(unused_imports)]
use std::path::MAIN_SEPARATOR_STR;

#[cfg(feature = "elasticsearch")]
fn create_elasticsearch() -> RunnableImage<elastic_search::ElasticSearch> {
    RunnableImage::from(elastic_search::ElasticSearch::default())
}

#[cfg(feature = "elasticsearch")]
#[cfg_attr(feature = "elasticsearch", tokio::test)]
async fn fiddler_elasticsearch_output_test() {
    let docker = Cli::default();
    let image = create_elasticsearch();
    let container = docker.run(image);
    let host_port = container.get_host_port_ipv4(9200);
    let config = format!(
        "input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.json
    codec: ToEnd
num_threads: 1
processors:
    - label: my_cool_mapping
      noop: {{}}
output:
  elasticsearch: 
    index: fiddler
    url: http://127.0.0.1:{host_port}"
    );

    let env = Runtime::from_config(&config).unwrap();
    env.run().await.unwrap();

    let sleep_time = std::time::Duration::from_secs(1);
    std::thread::sleep(sleep_time);

    let url = format!("http://127.0.0.1:{host_port}");
    let transport = Transport::single_node(&url).unwrap();
    let es_client = Elasticsearch::new(transport);
    let query = json!({
        "query": {
            "term": {
                "this": "is"
            }
        }
    });

    let mut response = es_client
        .search(SearchParts::Index(&["fiddler"]))
        .body(query)
        .pretty(true)
        .send()
        .await
        .unwrap();

    response = response.error_for_status_code().unwrap();

    let json: Value = response.json().await.unwrap();

    let results: Vec<&Value> = json["hits"]["hits"].as_array().unwrap().iter().collect();

    assert_eq!(results.len(), 1);
}
