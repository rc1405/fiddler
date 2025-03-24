use crate::config::register_plugin;
use crate::config::ItemType;
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::MessageBatch;
use crate::{Closer, Error, Processor};
use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Clone, Deserialize, Serialize)]
pub struct PyProcSpec {
    code: String,
    #[serde(rename = "string")]
    use_string: Option<bool>,
}

pub struct PyProc {
    code: String,
    use_string: bool,
}

#[async_trait]
impl Processor for PyProc {
    async fn process(&self, message: Message) -> Result<MessageBatch, Error> {
        Python::with_gil(|py| {
            let locals = PyDict::new_bound(py);
            if self.use_string {
                let raw_string = String::from_utf8(message.bytes)
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?;
                let python_string = PyString::new_bound(py, &raw_string);
                locals
                    .set_item("root", python_string)
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?;
            } else {
                let raw_bytes = message.bytes.as_slice();
                let python_bytes = PyBytes::new_bound(py, raw_bytes);
                locals
                    .set_item("root", python_bytes)
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?;
            };

            py.run_bound(&self.code.clone(), None, Some(&locals))
                .map_err(|e| Error::ProcessingError(format!("{}", e)))?;

            if self.use_string {
                let root: String = locals
                    .get_item("root")
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?
                    .ok_or(Error::ProcessingError("no root module found".into()))?
                    .extract()
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?;

                Ok(vec![Message {
                    bytes: root.as_bytes().into(),
                    metadata: message.metadata.clone(),
                }])
            } else {
                let root: Vec<u8> = locals
                    .get_item("root")
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?
                    .ok_or(Error::ProcessingError("no root module found".into()))?
                    .extract()
                    .map_err(|e| Error::ProcessingError(format!("{}", e)))?;

                Ok(vec![Message {
                    bytes: root,
                    metadata: message.metadata.clone(),
                }])
            }
        })
    }
}

impl Closer for PyProc {}

fn create_python(conf: &Value) -> Result<ExecutionType, Error> {
    let c: PyProcSpec = serde_yaml::from_value(conf.clone())?;
    let mut p = PyProc {
        code: c.code,
        use_string: false,
    };

    if let Some(b) = c.use_string {
        p.use_string = b;
    };

    Ok(ExecutionType::Processor(Box::new(p)))
}

pub(super) fn register_python() -> Result<(), Error> {
    let config = "type: object
properties:
  code: 
    type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin(
        "python".into(),
        ItemType::Processor,
        conf_spec,
        create_python,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_python().unwrap()
    }
}
