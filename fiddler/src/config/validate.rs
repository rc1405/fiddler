use super::{ItemType, ParsedRegisteredItem, RegisteredItem, ENV};
use crate::Error;
use serde_yaml::Value;
use std::collections::HashMap;
use tracing::trace;

/// The function takes the raw hashmap configuration item and looks up the registered
/// plugin and returns the ParsedRegisteredItem with the execution logic for used during
/// processing.
/// ```compile_fail
/// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
/// # use fiddler::modules::inputs;
/// # inputs::register_plugins().unwrap();
/// # use std::collections::HashMap;
/// use fiddler::config::parse_configuration_item;
///
/// use serde_yaml::Value;
/// let conf_str = r#"file:
///    filename: tests/data/input.txt
///    codec: ToEnd"#;
/// let parsed_config: HashMap<String, Value> = serde_yaml::from_str(conf_str).unwrap();
///
/// # tokio_test::block_on(async {
/// parse_configuration_item(ItemType::Input, &parsed_config).unwrap();
/// # })
/// ```
pub async fn parse_configuration_item(
    itype: ItemType,
    map: &HashMap<String, Value>,
) -> Result<ParsedRegisteredItem, Error> {
    let keys: Vec<String> = map.keys().cloned().collect();
    let first_key = keys.first().ok_or(Error::ConfigFailedValidation(format!(
        "unable to determine {} key",
        itype
    )))?;
    trace!("validationg item {} of type {}", first_key, itype);
    let item = get_item(&itype, first_key)?;

    let content = map
        .get(first_key)
        .ok_or(Error::ConfigFailedValidation(format!(
            "unable to validate {} key {}",
            itype, first_key
        )))?;

    let content_str = serde_yaml::to_string(content)?;
    item.format.validate(&content_str)?;
    trace!("Format for {} validated", first_key);
    Ok(ParsedRegisteredItem {
        creator: item.creator,
        config: content.clone(),
    })
}

fn get_item(itype: &ItemType, key: &String) -> Result<RegisteredItem, Error> {
    match ENV.lock() {
        Ok(lock) => {
            match lock.get(itype) {
                Some(i) => {
                    if let Some(item) = i.get(key) {
                        return Ok(item.clone());
                    }
                }
                None => return Err(Error::UnableToSecureLock),
            };
        }
        Err(_) => return Err(Error::UnableToSecureLock),
    };
    Err(Error::ConfigurationItemNotFound(key.clone()))
}
