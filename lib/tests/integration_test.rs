use fiddler::Environment;

mod dependencies;
use dependencies::{generator, processor, output};
use std::sync::Once;

static REGISTER: Once = Once::new();


#[tokio::test]
async fn end_to_end() {
    let config = "input:
  generator: 
    count: 5
pipeline:
    processors:
        - label: my_cool_mapping
          echo: {}
output:
  validate: 
    expected: 5
    prefix: echo";

    REGISTER.call_once(|| {
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_go_brrrt() {
    let config = "input:
  generator: 
    count: 5000
pipeline:
    processors:
        - label: my_cool_mapping
          echo: {}
output:
  validate: 
    expected: 5000
    prefix: echo";

    REGISTER.call_once(|| {
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_go_brrrrrrrrrt() {
    let config = "input:
  generator: 
    count: 10000
pipeline:
    processors:
        - label: my_cool_mapping
          echo: {}
output:
  validate: 
    expected: 10000
    prefix: echo";

    REGISTER.call_once(|| {
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_python_test() {
    let config = "input:
  generator: 
    count: 5
pipeline:
    processors:
        - label: my_cool_mapping
          python: 
            code: |
                import json
                decoded_string = root.decode(\"utf-8\")
                new_string = f\"python: {decoded_string}\"
                root = new_string.encode(\"utf-8\")
output:
  validate: 
    expected: 5
    prefix: python";

    REGISTER.call_once(|| {
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_python_string_test() {
    let config = "input:
  generator: 
    count: 5
pipeline:
    processors:
        - label: my_cool_mapping
          python: 
            string: true
            code: |
                import json
                new_string = f\"python: {root}\"
                root = new_string
output:
  validate: 
    expected: 5
    prefix: python";

    REGISTER.call_once(|| {
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}