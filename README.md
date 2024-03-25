# <img src="media/icon.png" alt="fiddler" width="80"/> Fiddler [![Crates.io][crates-badge]][crates-url] [![Apache 2.0 licensed][apache-badge]][apache-url] [![Documentation][doc-badge]][doc-url]

[crates-badge]: https://shields.io/crates/v/persistent-keystore-rs.svg
[crates-url]: https://crates.io/crates/persistent-keystore-rs
[apache-badge]: https://img.shields.io/badge/license-Apache2.0-blue.svg
[apache-url]: https://github.com/rc1405/persistent-keystore-rs/blob/main/LICENSE
[doc-badge]: https://img.shields.io/badge/docs-API%20Docs-green.svg
[doc-url]: https://docs.rs/persistent-keystore-rs/latest/persistent-keystore-rs

<br>
Fiddler is a stream processor built in rust and inspired by Benthos.  If you haven't seen [Benthos, go check it out!](https://github.com/benthosdev/benthos/tree/main).  Like Benthos, Fiddler using a yaml based configuration file syntax to map together inputs, processors, and outputs.  
<br>
<br>
Such as:
<br>
<br>

```
input:
    stdin: {}
pipeline:
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

# Build
Build with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) `cargo build --release`

# Plugins
`TODO`

# Creating your plugins
`TODO`

# Contributing
`TODO` 