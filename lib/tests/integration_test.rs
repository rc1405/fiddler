#![allow(dead_code)]
use fiddler::Environment;

mod dependencies;
use dependencies::{generator, processor, output, jsongenerator, mock};
use std::sync::Once;
use std::path::MAIN_SEPARATOR_STR;

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
    expected: 
      - 'echo: Hello World 4'
      - 'echo: Hello World 3'
      - 'echo: Hello World 2'
      - 'echo: Hello World 1'
      - 'echo: Hello World 0'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}


#[cfg_attr(feature = "python", tokio::test)]
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
    expected: 
      - 'python: Hello World 4'
      - 'python: Hello World 3'
      - 'python: Hello World 2'
      - 'python: Hello World 1'
      - 'python: Hello World 0'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
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
        - label: my_cool_mapping
          echo: {}
output:
  validate: 
    expected: 
      - 'echo: python: Hello World 4'
      - 'echo: python: Hello World 3'
      - 'echo: python: Hello World 2'
      - 'echo: python: Hello World 1'
      - 'echo: python: Hello World 0'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_check_test() {
    let config = "input:
  json_generator: 
    count: 5
pipeline:
    processors:
        - switch:
            - check: 
                condition: '\"Hello World\" > `5`'
                processors:
                  - python: 
                        string: true
                        code: |
                            import json
                            new_string = f\"python: {root}\"
                            root = new_string
            - label: my_cool_mapping
              echo: {}
output:
  validate: 
    expected: 
      - 'echo: {\"Hello World\": 4}'
      - 'echo: {\"Hello World\": 3}'
      - 'echo: {\"Hello World\": 2}'
      - 'echo: {\"Hello World\": 1}'
      - 'echo: {\"Hello World\": 0}'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_check_many_procs_test() {
    let config = "input:
  json_generator: 
    count: 5
pipeline:
    processors:
        - switch:
            - check: 
                condition: '\"Hello World\" <= `5`'
                processors:
                  - python: 
                        string: true
                        code: |
                            import json
                            new_string = f\"python: {root}\"
                            root = new_string
                  - echo: {}
            - label: my_cool_mapping
              echo: {}
output:
  validate: 
    expected: 
      - 'echo: python: {\"Hello World\": 4}'
      - 'echo: python: {\"Hello World\": 3}'
      - 'echo: python: {\"Hello World\": 2}'
      - 'echo: python: {\"Hello World\": 1}'
      - 'echo: python: {\"Hello World\": 0}'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_output_test() {
    let config = "input:
  json_generator: 
    count: 5
pipeline:
    processors:
    - label: my_cool_mapping
      python: 
        string: true
        code: |
            import json
            msg = json.loads(root)
            msg['Python'] = 'rocks'
            root = json.dumps(msg)
output:
  switch:
    - check:
        condition: '\"Hello World\" > `5`'
        output:
          validate: 
            expected: []
    - validate:
        expected: 
          - '{\"Hello World\": 4, \"Python\": \"rocks\"}'
          - '{\"Hello World\": 3, \"Python\": \"rocks\"}'
          - '{\"Hello World\": 2, \"Python\": \"rocks\"}'
          - '{\"Hello World\": 1, \"Python\": \"rocks\"}'
          - '{\"Hello World\": 0, \"Python\": \"rocks\"}'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_file_reader_test() {
    let config = format!("input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt
    codec: Lines
pipeline:
  processors:
    - label: my_cool_mapping
      noop: {{}}
output:
  validate:
    expected: 
      - Hello World
      - This is the end");

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(&config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_file_reader_test_full() {
    let config = format!("input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt
    codec: ToEnd
pipeline:
  processors:
    - label: my_cool_mapping
      lines: {{}}
output:
  validate:
    expected: 
      - Hello World
      - This is the end");

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(&config).unwrap();
    env.run().await.unwrap();
}