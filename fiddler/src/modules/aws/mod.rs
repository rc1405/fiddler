use crate::Error;
use serde::Deserialize;
mod s3;
mod sqs;

#[derive(Deserialize, Clone)]
pub(crate) struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}
pub(crate) fn register_plugins() -> Result<(), Error> {
    sqs::register_sqs()?;
    s3::register_s3()?;
    Ok(())
}
