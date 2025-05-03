//! Fast and flexible data stream processor written in Rust
//!
//! Provides a cli for running, linting and testing data streaming pipelines
//! using a declaritive yaml based configuration for data aggregation and
//! transformation
use fiddler_cmd::run;
use fiddler::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run().await
}