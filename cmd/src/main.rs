use fiddler::Error;
use fiddler::Environment;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = "input:
    stdin: {}
pipeline:
    processors:
        - label: my_cool_mapping
          noop: {}
        - label: my_cool_mapping
          noop: {}
        - label: my_cool_mapping
          noop: {}
output:
    stdout: {}";

    let env = Environment::from_config(config)?;
    env.run().await?;

    Ok(())
}
