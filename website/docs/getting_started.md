# Get Started

## Running as a CLI

### Installation

1. Grab a release for your OS [here](https://github.com/rc1405/fiddler/releases)
1. Install with Cargo `cargo install fiddler`
1. Build with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) `cargo build --release --features all`

### Usage

```
Usage: fiddler <COMMAND>

Commands:
  lint  Data Stream processor CLI written in rust
  run   Data Stream processor CLI written in rust
  test  Data Stream processor CLI written in rust
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### Run
```
Data Stream processor CLI written in rust

Usage: fiddler run [OPTIONS]

Options:
  -c, --config <CONFIG>
  -l, --log-level <LOG_LEVEL>  [default: none] [possible values: info, debug, trace, error, none]
  -h, --help                   Print help
  -V, --version                Print version
```

1. `fiddler run -c <path_to_config_file> [ -c ... ]`

#### Lint
```
Data Stream processor CLI written in rust

Usage: fiddler lint [OPTIONS]

Options:
  -c, --config <CONFIG>
  -h, --help             Print help
  -V, --version          Print version
```

1. `fiddler lint -c <path_to_config_file> [ -c ... ]`

#### Test
```
Data Stream processor CLI written in rust

Usage: fiddler test [OPTIONS]

Options:
  -c, --config <CONFIG>
  -l, --log-level <LOG_LEVEL>  [default: none] [possible values: info, debug, trace, error, none]
  -h, --help                   Print help
  -V, --version                Print version
```

Tests within fiddler are designed to test the processing pipeline configuration.  The expected naming syntax is `<filename>_test.yaml`.  I.e. if you have a configuration `input.yaml`.  The expected filename is `input_test.yaml`.  

The test file syntax is as follows:  

```yml
- name: name_of_test
  inputs:
   - list of expected input strings
  expected_outputs:
   - list of expected output strings
```

For example:  
```yml
- name: input_test
  inputs:
    - Hello World
  expected_outputs: 
    - Hello World
```

1. `fiddler-cli test -c <path_to_configuration>.yaml`

## Integration

### Building own CLI with Additional modules

Creating your own fiddler-cli requires a binary crate with at least two dependencies, `fiddler_cmd` and `fiddler`.

> `cargo new my-cli --bin`  
> `cargo add fiddler`  
> `cargo add fiddler_cmd`  

Next create your runtime, by default in `src/main.rs`
```rust
use fiddler_cmd::run;
use fiddler::Error;
use fiddler::config::{ConfigSpec, register_plugin};

#[tokio::main]
async fn main() -> Result<(), Error> {
    // insert registration of custom modules here
    register_plugin("mock".into(), ItemType::Input, conf_spec, create_mock_input)?;

    // run the normal CLI
    run().await
}
```

### Integrating the runtme into your own application
Head on over to [docs.rs](https://docs.rs/fiddler/latest/fiddler) to view the latest API reference for utilizing fiddler runtime.  