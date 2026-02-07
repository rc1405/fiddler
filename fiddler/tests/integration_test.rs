#![allow(unused_crate_dependencies)]
#![allow(missing_docs)]
#![allow(dead_code)]
use fiddler::Runtime;

mod dependencies;
use dependencies::{generator, jsongenerator, mock, output, processor};
use std::path::MAIN_SEPARATOR_STR;
use std::sync::Once;

static REGISTER: Once = Once::new();

#[tokio::test]
async fn fiddler_single_event() {
    let config = "input:
  generator: 
    count: 1
num_threads: 1
processors:
  - label: my_cool_mapping
    noop: {}
output:
  validate:
    expected: 
      - 'Hello World 0'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    std::env::set_var("TestingEnvVarReplacement", "ReplacementSuccessful");

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn end_to_end() {
    let config = "input:
  generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_python_test() {
    let config = "input:
  generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_python_string_test() {
    let config = "input:
  generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_check_test() {
    let config = "input:
  json_generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_check_many_procs_test() {
    let config = "input:
  json_generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_switch_output_test() {
    let config = "input:
  json_generator: 
    count: 5
num_threads: 1
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

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_file_reader_test() {
    let config = format!(
        "input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt
    codec: Lines
num_threads: 1
processors:
  - label: my_cool_mapping
    noop: {{}}
output:
  validate:
    expected: 
      - Hello World
      - This is the end"
    );

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(&config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_file_reader_test_full() {
    let config = format!(
        "input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.txt
    codec: ToEnd
num_threads: 1
processors:
  - label: my_cool_mapping
    lines: {{}}
output:
  validate:
    expected: 
      - Hello World
      - This is the end"
    );

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(&config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn fiddler_file_reader_test_full_json() {
    let expected_output = "{\"this\": \"is\", \"a\": \"testing\", \"document\": true}";
    let config = format!(
        "input:
  file: 
    filename: tests{MAIN_SEPARATOR_STR}data{MAIN_SEPARATOR_STR}input.json
    codec: ToEnd
num_threads: 1
processors:
  - label: my_cool_mapping
    lines: {{}}
output:
  validate:
    expected: 
      - |
        {expected_output}",
    );

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(&config).await.unwrap();
    env.run().await.unwrap();
}

#[cfg_attr(feature = "python", tokio::test)]
async fn fiddler_env_replacement_test() {
    let config = "input:
  json_generator: 
    count: 1
num_threads: 1
processors:
  - label: my_cool_mapping
    python: 
      string: true
      code: |
        root = '{{ TestingEnvVarReplacement }}'
output:
  validate:
    expected: 
      - ReplacementSuccessful";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    std::env::set_var("TestingEnvVarReplacement", "ReplacementSuccessful");

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn decompression() {
    let config = "input:
  mock_input: 
    input:
      - H4sIAAAAAAAC//NIzcnJVwjPL8pJAQBWsRdKCwAAAA==
num_threads: 1
processors:
  - decode: {}
  - decompress: {}
output:
  validate: 
    expected: 
      - 'Hello World'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn processor_try_catch() {
    let config = "input:
  mock_input:
    input:
      - SGVsbG8gV29ybGQ=
      - H4sIAAAAAAAC//NIzcnJVwjPL8pJAQBWsRdKCwAAAA==
num_threads: 1
processors:
  - decode: {}
  - try:
      processor:
        decompress: {}
      catch:
        - noop: {}
output:
  validate:
    expected:
      - 'Hello World'
      - 'Hello World'";

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

// ============================================================================
// Filter Processor Integration Tests
// ============================================================================

#[tokio::test]
async fn filter_passes_matching_messages() {
    let config = r#"input:
  mock_input:
    input:
      - '{"status": "active", "name": "Alice"}'
      - '{"status": "active", "name": "Bob"}'
num_threads: 1
processors:
  - filter:
      condition: "status == 'active'"
output:
  validate:
    expected:
      - '{"status": "active", "name": "Alice"}'
      - '{"status": "active", "name": "Bob"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_drops_non_matching_messages() {
    // Only active messages should pass through
    let config = r#"input:
  mock_input:
    input:
      - '{"status": "active", "name": "Alice"}'
      - '{"status": "inactive", "name": "Bob"}'
      - '{"status": "active", "name": "Charlie"}'
num_threads: 1
processors:
  - filter:
      condition: "status == 'active'"
output:
  validate:
    expected:
      - '{"status": "active", "name": "Alice"}'
      - '{"status": "active", "name": "Charlie"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_numeric_comparison() {
    let config = r#"input:
  mock_input:
    input:
      - '{"name": "Alice", "age": 25}'
      - '{"name": "Bob", "age": 17}'
      - '{"name": "Charlie", "age": 30}'
num_threads: 1
processors:
  - filter:
      condition: "age >= `18`"
output:
  validate:
    expected:
      - '{"name": "Alice", "age": 25}'
      - '{"name": "Charlie", "age": 30}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_nested_field() {
    let config = r#"input:
  mock_input:
    input:
      - '{"user": {"verified": true, "name": "Alice"}}'
      - '{"user": {"verified": false, "name": "Bob"}}'
num_threads: 1
processors:
  - filter:
      condition: "user.verified == `true`"
output:
  validate:
    expected:
      - '{"user": {"verified": true, "name": "Alice"}}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_array_contains() {
    let config = r#"input:
  mock_input:
    input:
      - '{"tags": ["urgent", "important"]}'
      - '{"tags": ["normal", "review"]}'
      - '{"tags": ["important", "review"]}'
num_threads: 1
processors:
  - filter:
      condition: "contains(tags, 'important')"
output:
  validate:
    expected:
      - '{"tags": ["urgent", "important"]}'
      - '{"tags": ["important", "review"]}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_complex_condition() {
    let config = r#"input:
  mock_input:
    input:
      - '{"type": "order", "total": 150, "status": "pending"}'
      - '{"type": "order", "total": 50, "status": "pending"}'
      - '{"type": "order", "total": 200, "status": "cancelled"}'
      - '{"type": "invoice", "total": 300, "status": "pending"}'
num_threads: 1
processors:
  - filter:
      condition: "type == 'order' && total > `100` && status != 'cancelled'"
output:
  validate:
    expected:
      - '{"type": "order", "total": 150, "status": "pending"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn filter_all_dropped() {
    // All messages should be filtered out
    let config = r#"input:
  mock_input:
    input:
      - '{"status": "inactive"}'
      - '{"status": "pending"}'
num_threads: 1
processors:
  - filter:
      condition: "status == 'active'"
output:
  validate:
    expected: []"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

// ============================================================================
// Transform Processor Integration Tests
// ============================================================================

#[tokio::test]
async fn transform_single_field() {
    // Single field mapping - no ordering issues
    let config = r#"input:
  mock_input:
    input:
      - '{"firstName": "Alice", "lastName": "Smith"}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "firstName"
          target: "name"
output:
  validate:
    expected:
      - '{"name":"Alice"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_nested_single_field() {
    let config = r#"input:
  mock_input:
    input:
      - '{"user": {"profile": {"email": "alice@example.com"}, "name": "Alice"}}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "user.profile.email"
          target: "email"
output:
  validate:
    expected:
      - '{"email":"alice@example.com"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_array_projection() {
    let config = r#"input:
  mock_input:
    input:
      - '{"users": [{"name": "Alice"}, {"name": "Bob"}]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "users[*].name"
          target: "names"
output:
  validate:
    expected:
      - '{"names":["Alice","Bob"]}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_jmespath_length() {
    let config = r#"input:
  mock_input:
    input:
      - '{"items": ["a", "b", "c"]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "length(items)"
          target: "count"
output:
  validate:
    expected:
      - '{"count":3}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_jmespath_join() {
    let config = r#"input:
  mock_input:
    input:
      - '{"items": ["a", "b", "c"]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "join(', ', items)"
          target: "joined"
output:
  validate:
    expected:
      - '{"joined":"a, b, c"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_array_first_element() {
    let config = r#"input:
  mock_input:
    input:
      - '{"items": ["first", "second", "third"]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "items[0]"
          target: "first"
output:
  validate:
    expected:
      - '{"first":"first"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_array_last_element() {
    let config = r#"input:
  mock_input:
    input:
      - '{"items": ["first", "second", "third"]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "items[-1]"
          target: "last"
output:
  validate:
    expected:
      - '{"last":"third"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_filter_expression() {
    let config = r#"input:
  mock_input:
    input:
      - '{"products": [{"name": "A", "price": 5}, {"name": "B", "price": 15}, {"name": "C", "price": 25}]}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "products[?price > `10`].name"
          target: "expensive"
output:
  validate:
    expected:
      - '{"expensive":["B","C"]}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

#[tokio::test]
async fn transform_multiple_messages_single_field() {
    let config = r#"input:
  mock_input:
    input:
      - '{"id": 1, "value": "first"}'
      - '{"id": 2, "value": "second"}'
      - '{"id": 3, "value": "third"}'
num_threads: 1
processors:
  - transform:
      mappings:
        - source: "id"
          target: "record_id"
output:
  validate:
    expected:
      - '{"record_id":1}'
      - '{"record_id":2}'
      - '{"record_id":3}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}

// ============================================================================
// Filter + Transform Combined Tests
// ============================================================================

#[tokio::test]
async fn filter_then_transform() {
    let config = r#"input:
  mock_input:
    input:
      - '{"status": "active", "user": {"name": "Alice", "email": "alice@example.com"}}'
      - '{"status": "inactive", "user": {"name": "Bob", "email": "bob@example.com"}}'
      - '{"status": "active", "user": {"name": "Charlie", "email": "charlie@example.com"}}'
num_threads: 1
processors:
  - filter:
      condition: "status == 'active'"
  - transform:
      mappings:
        - source: "user.name"
          target: "name"
output:
  validate:
    expected:
      - '{"name":"Alice"}'
      - '{"name":"Charlie"}'"#;

    REGISTER.call_once(|| {
        mock::register_mock_input().unwrap();
        jsongenerator::register_json_generator().unwrap();
        generator::register_generator().unwrap();
        processor::register_echo().unwrap();
        output::register_validate().unwrap();
    });

    let env = Runtime::from_config(config).await.unwrap();
    env.run().await.unwrap();
}
