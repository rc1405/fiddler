use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use crate::{Error, Processor, Closer};
use crate::config::{ConfigSpec, ExecutionType};
use crate::config::register_plugin;
use crate::config::ItemType;
use crate::Message;
use crate::MessageBatch;
use serde_yaml::Value;
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
pub struct PyProcSpec {
    code: String,
    #[serde(rename="string")]
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
            let locals = PyDict::new(py);
            if self.use_string {
                let raw_string = String::from_utf8(message.bytes).map_err(|e| Error::ProcessingError(format!("{}", e)))?;
                let python_string = PyString::new(py, &raw_string);
                locals.set_item("root", python_string).map_err(|e| Error::ProcessingError(format!("{}", e)))?;
            } else {
                let raw_bytes = message.bytes.as_slice();
                let python_bytes = PyBytes::new(py, raw_bytes);
                locals.set_item("root", python_bytes).map_err(|e| Error::ProcessingError(format!("{}", e)))?;
            };

            py.run(
                &self.code.clone(),
                None,
                Some(locals),
            ).map_err(|e| Error::ProcessingError(format!("{}", e)))?;

            let root = locals.get_item("root").map_err(|e| Error::ProcessingError(format!("{}", e)))?
                .ok_or(Error::ProcessingError("no root module found".into()))?;

            if self.use_string {
                let result: &PyString = root.downcast().map_err(|e| Error::ProcessingError(format!("{}", e)))?;
                let vec_str = result.to_str().map_err(|e| Error::ProcessingError(format!("{}", e)))?;

                Ok(vec![Message{
                    bytes: vec_str.as_bytes().into(),
                    metadata: message.metadata.clone(),
                }])
            } else {
                
                let result: &PyBytes = root.downcast().map_err(|e| Error::ProcessingError(format!("{}", e)))?;

                Ok(vec![Message{
                    bytes: result.as_bytes().to_vec(),
                    metadata: message.metadata.clone(),
                }])
            }
            
        })
    }
}

impl Closer for PyProc {
    fn close(&self) -> Result<(), Error> {
        Ok(())
    }
}

fn create_python(conf: &Value) -> Result<ExecutionType, Error> {
    let c: PyProcSpec = serde_yaml::from_value(conf.clone())?;
    let mut p = PyProc{
        code: c.code,
        use_string: false
    };
    
    if let Some(b) = c.use_string {
        p.use_string = b;
    };

    Ok(ExecutionType::Processor(Arc::new(p)))
}

// #[cfg_attr(feature = "python", fiddler_registration_func)]
pub fn register_python() -> Result<(), Error> {
    let config = "type: object
properties:
  code: 
    type: string";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("python".into(), ItemType::Processor, conf_spec, create_python)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn register_plugin() {
        register_python().unwrap()
    }
}