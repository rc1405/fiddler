# Get Started

## Running as a CLI

### Installation

1. Grab a release for your OS [here](https://github.com/rc1405/fiddler/releases)
1. Install with Cargo `cargo install fiddler`
1. Build with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) `cargo build --release --features all`

### Running
#### Run
1. `fiddler run -c <path_to_config_file> [ -c ... ]`

#### Lint
1. `fiddler lint -c <path_to_config_file> [ -c ... ]`

#### Test
Tests are expected in the format of `<filename>_test.yaml`.  I.e. if you have a configuration `input.yaml`.  The expected filename is `input_test.yaml`.  The test file syntax is as follows:
```
- name: name_of_test
  inputs:
   - list of expected input strings
  expected_outputs:
   - list of expected output strings
```

Tests can be run as `fiddler-cli test -c <path_to_configuration>.yaml`

## Integration
Head on over to [docs.rs](https://docs.rs/fiddler/latest/fiddler) to view the latest API reference for utilizing fiddler within your own applications.  