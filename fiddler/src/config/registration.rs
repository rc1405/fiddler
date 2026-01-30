use super::{Callback, ConfigSpec, ItemType, RegisteredItem, ENV};
use crate::Error;
use tracing::{debug, error};

/// Validates the configuration object has valid and registered inputs, outputs, and processors.
/// Note: Plugins must be registered with the environment prior to calling validate.  This is
/// automatically done when using Environment
/// ```compile_fail
/// # use fiddler::config::{ConfigSpec, ItemType, ExecutionType};
/// # use fiddler::modules::processors::noop::NoOp;
/// # use std::sync::Arc;
/// use fiddler::config::register_plugin;
///
/// let conf_str = r#"type: object"#;
/// let conf_spec = ConfigSpec::from_schema(conf_str).unwrap();
///
/// register_plugin("noop".into(), ItemType::Processor, conf_spec, |v| {
///     Ok(ExecutionType::Processor(Box::new(NoOp{})))
/// }).unwrap();
/// ```
pub fn register_plugin(
    name: String,
    itype: ItemType,
    format: ConfigSpec,
    creator: Callback,
) -> Result<(), Error> {
    let r = RegisteredItem { creator, format };

    match ENV.write() {
        Ok(mut lock) => {
            match lock.get_mut(&itype) {
                Some(i) => {
                    if i.insert(name.clone(), r).is_some() {
                        error!(name = name.clone(), "plugin is already registered");
                        return Err(Error::DuplicateRegisteredName(name));
                    };
                    debug!(
                        name = name.clone(),
                        plugin_type = format!("{itype}"),
                        "plugin registered"
                    );
                }
                None => {
                    error!(kind = "unable to borrow mut", "InternalServerError");
                    return Err(Error::UnableToSecureLock);
                }
            };
        }
        Err(_) => {
            error!(kind = "unable to secure write lock", "InternalServerError");
            return Err(Error::UnableToSecureLock);
        }
    };

    Ok(())
}
