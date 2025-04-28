use crate::config::register_plugin;
use crate::config::{parse_configuration_item, Item, ItemType};
use crate::config::{ConfigSpec, ExecutionType};
use crate::Message;
use crate::{Closer, Error, Output};
use async_trait::async_trait;
use fiddler_macros::fiddler_registration_func;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Deserialize, Serialize)]
struct CheckConfig {
    label: Option<String>,
    condition: String,
    output: Item,
}

pub struct Check {
    _label: Option<String>,
    condition: String,
    output: Box<dyn Output + Send + Sync>,
}

fn perform_check(condition: &str, json_str: String) -> Result<(), Error> {
    let mut runtime = jmespath::Runtime::new();
    runtime.register_builtin_functions();
    let expr = runtime
        .compile(condition)
        .map_err(|e| Error::ProcessingError(e.to_string()))?;
    let data = jmespath::Variable::from_json(&json_str)
        .map_err(|e| Error::ProcessingError(e.to_string()))?;

    let result = expr
        .search(data)
        .map_err(|e| Error::ProcessingError(format!("{}", e)))?;
    if !result.as_boolean().unwrap_or(false) {
        return Err(Error::ConditionalCheckfailed);
    };
    Ok(())
}

#[async_trait]
impl Output for Check {
    async fn write(&mut self, message: Message) -> Result<(), Error> {
        let json_str = String::from_utf8(message.bytes.clone())
            .map_err(|e| Error::ProcessingError(e.to_string()))?;

        perform_check(&self.condition, json_str)?;

        self.output.write(message).await?;
        Ok(())
    }
}

#[async_trait]
impl Closer for Check {
    async fn close(&mut self) -> Result<(), Error> {
        self.output.close().await
    }
}

#[fiddler_registration_func]
fn create_check(conf: Value) -> Result<ExecutionType, Error> {
    let c: CheckConfig = serde_yaml::from_value(conf.clone())?;
    let _ = jmespath::compile(&c.condition)
        .map_err(|e| Error::ConfigFailedValidation(format!("{}", e)))?;

    let ri = parse_configuration_item(ItemType::Output, &c.output.extra).await?;

    let step = ((ri.creator)(ri.config.clone())).await?;
    let out = match step {
        ExecutionType::Output(o) => o,
        _ => {
            return Err(Error::ConfigFailedValidation(
                "output must be a valid output".into(),
            ))
        }
    };

    let s = Check {
        _label: c.label,
        condition: c.condition,
        output: out,
    };

    Ok(ExecutionType::Output(Box::new(s)))
}

pub(super) fn register_check() -> Result<(), Error> {
    let config = "type: object
properties:
  label:
    type: string
  condition:
    type: string
  output:
    type: object";
    let conf_spec = ConfigSpec::from_schema(config)?;

    register_plugin("check".into(), ItemType::Output, conf_spec, create_check)
}
