use crate::Error;

pub mod inputs;
pub mod metrics;
pub mod outputs;
pub mod processors;

#[cfg(feature = "aws")]
pub mod aws;

pub(crate) fn register_plugins() -> Result<(), Error> {
    inputs::register_plugins()?;
    processors::register_plugins()?;
    outputs::register_plugins()?;
    metrics::register_plugins()?;

    #[cfg(feature = "aws")]
    aws::register_plugins()?;

    Ok(())
}
