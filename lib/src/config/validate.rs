use super::{ExecutionType, ItemType, ParsedRegisteredItem, RegisteredItem, ENV};
use crate::Error;
use serde_yaml::Value;
use std::collections::HashMap;

/// The function takes the raw hashmap configuration item and looks up the registered
/// plugin and returns the ParsedRegisteredItem with the execution logic for used during
/// processing.
/// ```
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
/// parse_configuration_item(ItemType::Input, &parsed_config).unwrap();
/// ```
pub fn parse_configuration_item(
    itype: ItemType,
    map: &HashMap<String, Value>,
) -> Result<ParsedRegisteredItem, Error> {
    let keys: Vec<String> = map.keys().cloned().collect();
    let first_key = keys.first().ok_or(Error::ConfigFailedValidation(format!(
        "unable to determine {} key",
        itype
    )))?;
    let item = get_item(&itype, first_key)?;

    let content = map
        .get(first_key)
        .ok_or(Error::ConfigFailedValidation(format!(
            "unable to validate {} key {}",
            itype, first_key
        )))?;
    let content_str = serde_yaml::to_string(content)?;
    item.format.validate(&content_str)?;
    match itype {
        ItemType::Input => {
            let creator = (item.creator)(content)?;
            match &creator {
                ExecutionType::Input(_) => {}
                _ => {
                    return Err(Error::ConfigFailedValidation(
                        "invalid type returned for input".into(),
                    ))
                }
            };

            Ok(ParsedRegisteredItem {
                execution_type: creator,
                item_type: ItemType::Input,
            })
        }
        ItemType::InputBatch => Err(Error::NotYetImplemented),
        ItemType::Output => {
            let creator = (item.creator)(content)?;
            match &creator {
                ExecutionType::Output(_) => {}
                _ => {
                    return Err(Error::ConfigFailedValidation(
                        "invalid type returned for output".into(),
                    ))
                }
            };

            Ok(ParsedRegisteredItem {
                execution_type: creator,
                item_type: ItemType::Output,
            })
        }
        ItemType::OutputBatch => Err(Error::NotYetImplemented),
        ItemType::Processor => {
            let creator = (item.creator)(content)?;
            match &creator {
                ExecutionType::Processor(_) => {}
                _ => {
                    return Err(Error::ConfigFailedValidation(
                        "invalid type returned for processor".into(),
                    ))
                }
            };

            Ok(ParsedRegisteredItem {
                execution_type: creator,
                item_type: ItemType::Processor,
            })
        }
    }
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
