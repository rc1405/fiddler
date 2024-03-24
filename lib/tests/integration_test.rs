use fiddler::Environment;

mod dependencies;
use dependencies::{generator, processor, output, jsongenerator};
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
        jsongenerator::register_json_generator().unwrap();
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
        jsongenerator::register_json_generator().unwrap();
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
        jsongenerator::register_json_generator().unwrap();
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
        - label: my_cool_mapping
          echo: {}
output:
  validate: 
    expected: 5
    prefix: python";

    REGISTER.call_once(|| {
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
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
    expected: 5
    prefix: echo";

    REGISTER.call_once(|| {
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
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
    expected: 5
    prefix: 'echo: python'";

    REGISTER.call_once(|| {
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
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
        condition: '\"Hellow World\" > `5`'
        output:
          validate: 
            expected: 5
            prefix: echo
    - validate:
        expected: 5
        prefix: 'python'";

    REGISTER.call_once(|| {
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });
    
    let env = Environment::from_config(config).unwrap();
    env.run().await.unwrap();
}