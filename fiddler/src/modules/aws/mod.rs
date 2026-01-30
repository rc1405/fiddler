use crate::Error;
use serde::Deserialize;
pub mod cloudwatch;
mod s3;
mod sqs;

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}
pub(crate) fn register_plugins() -> Result<(), Error> {
    sqs::register_sqs()?;
    s3::register_s3()?;
    cloudwatch::register_cloudwatch()?;
    Ok(())
}
