# <img src="media/icon.png" alt="fiddler" width="80"/> Fiddler [![Crates.io][crates-badge]][crates-url] [![Apache 2.0 licensed][apache-badge]][apache-url] [![Documentation][doc-badge]][doc-url]

[crates-badge]: https://shields.io/crates/v/fiddler.svg
[crates-url]: https://crates.io/crates/fiddler
[apache-badge]: https://img.shields.io/badge/license-Apache2.0-blue.svg
[apache-url]: https://github.com/rc1405/fiddler/blob/main/LICENSE
[doc-badge]: https://img.shields.io/badge/docs-API%20Docs-green.svg
[doc-url]: https://docs.rs/fiddler/latest/fiddler

<br>
Fiddler is a stream processor built in rust that uses a yaml based configuration file syntax to map together inputs, processors, and outputs. 

<br>
<br>
Such as:
<br>
<br>

```
input:
    stdin: {}
pipeline:
  max_in_flight: 10
  processors:
    - python: 
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
    - stdout: {}
```

The main inline message manipulation is provided by Python, and requires the Python shared library.

To install the Python shared library on Ubuntu:
`sudo apt install python3-dev`
To install the Python shared library on RPM based distributions (e.g. Fedora, Red Hat, SuSE), install the `python3-devel` package.

Any third party packages utilized in Python will have to be installed on the OS running fiddler.  

The message format in and out is simple, the source of the event is stored in the local variable named `root`; and the output taken from executing the code is expected to be named `root`.  By default `root` is bytes of the message, unless the argument `string: true` is proved, in which case `root` is converted to a string.
<br>
<br>
Conditional execution of steps for processing, or selection of outputs can be obtained through the use of `check` and `switch` plugins.  For processing, it looks like:
<br>
```
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
```
<br>
The condition format utilized [jmespath](https://jmespath.org/specification.html) syntax for evaluation.  As such, this can only be utilized with JSON documents.

# Install
1. Grab a release for your OS [here](https://github.com/rc1405/fiddler/releases)
1. Install with Cargo `cargo install fiddler`

# Run
1. `fiddler run -c <path_to_config_file> [ -c ... ]`

# Lint
1. `fiddler lint -c <path_to_config_file> [ -c ... ]`

# Test
Tests are expected in the format of `<filename>_test.yaml`.  I.e. if you have a configuration `input.yaml`.  The expected filename is `input_test.yaml`.  The test file syntax is as follows:
```
- name: name_of_test
  inputs:
   - list of expected input strings
  expected_outputs:
   - list of expected output strings
```

Tests can be run as `fiddler-cli test -c <path_to_configuration>.yaml`

# Build
Build with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) `cargo build --release --features all`

# Plugins
## Input
### File
```
input:
  file: 
    filename: tests/data/input.txt
    codec: ToEnd
```
* `filename`: `string`: path to file to be read
* `codec`:  `string`: Possible Values: `ToEnd`: Read entire file in one message.  `Lines`: Read file line by line

### Stdin
```
input:
  stdin: {}
```
## Processors
### Lines
```
processors:
  - lines: {}
```
### NoOp
```
processors:
  - noop: {}
```
### Python
```
processors:
  - python: 
      string: true
      code: |
          import json
          msg = json.loads(root)
          msg['Python'] = 'rocks'
          root = json.dumps(msg)
```
* `string`: `bool`: whether or not the message contents are passed as a string or bytes (default)
* `code`: `string`: python code to execute
### Check
```
processors:
  - check: 
      condition: '\"Hello World\" <= `5`'
      processors:
        - python: 
            string: true
            code: |
              import json
              new_string = f\"python: {root}\"
              root = new_string
```
* `condition`: `string`: jmespath expression to evaluate if processors are run
* `processors`: `list`: list of processors to run if condition is met
### Switch
```
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
      - echo: {}
```
* `array`: array of processors to run, typically used with the `check` processor.  once a processor is successful, no other processors are ran
## Output
### StdOut
```
output:
  stdout: {}
```
### Check
```
output:
  check:
    condition: '\"Hello World\" > `5`'
    output:
      stdout: {}
```
* `condition`: `string`: jmespath expression to evaluate if processors are run
* `output`: `object`: output to use if check is true
### Switch
```
output:
  switch:
    - check:
        condition: '\"Hello World\" > `5`'
        output:
          validate: 
            expected: []
```
* `array`: array of outputs, typically used with the `check` output.  

### Elasticsearch
```
output:
  elasticsearch:
    url: http://example.com:9200
    username: user
    password: password
    cloud_id: some_cloud_id
    index: example
```
* `url`: if not using elasticsearch cloud, specify the url of the es cluster. i.e. http://127.0.0.1:9200
* `username`: username for authentication
* `password`: password for authentication
* `cloud_id`: if using elasticsearch cloud, specify the cloud_id
* `index`: index to insert events into

**Note:**  Variables can be collected from the environment, so when providing password, you may set it to `password: "{{ ElasticSearchPassword }}"` and ElasticSearchPassword will be pulled from the environment variables and provided to the configuration.

# Contributing

Contributions are welcome and encouraged! Contributions
come in many forms. You could:

  1. Submit a feature request or bug report as an [issue].
  2. Ask for improved documentation as an [issue].
  3. Comment on [issues that require feedback].
  4. Contribute code via [pull requests].

[issue]: https://github.com/rc1405/fiddler/issues
[issues that require feedback]: https://github.com/rc1405/fiddler/issues?q=is%3Aopen+is%3Aissue+label%3A%22feedback+wanted%22
[pull requests]: https://github.com/rc1405/fiddler/pulls

Note that unless you explicitly state otherwise, any contribution intentionally 
submitted for inclusion in fiddler by you shall be licensed under the
Apache License, Version 2.0, without any additional terms or conditions.

# Known Issues
* Cargo shows warnings on switch plugins when compiled on Windows that they are skipped when in fact they are not.
* build.rs doesn't validate Python is present when compiled on Windows
* Integration tests failing on windows GHA

// Needed Test Cases
OutputBatch provided as Output doesn't elevate the error
[Done] CLI Test should only use one thread
Add thread limit on output
[Done] Update doc tests
[Done] Fix test yamls
Improve Logging
Add Metrics
Fix warnings