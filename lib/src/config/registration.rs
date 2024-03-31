use super::{Callback, ConfigSpec, ItemType, RegisteredItem, ENV};
use crate::Error;
use tracing::{debug, error};

pub fn register_plugin(
    name: String,
    itype: ItemType,
    format: ConfigSpec,
    creator: Callback,
) -> Result<(), Error> {
    let r = RegisteredItem { creator, format };

    match ENV.lock() {
        Ok(mut lock) => {
            match lock.get_mut(&itype) {
                Some(i) => {
                    if i.insert(name.clone(), r).is_some() {
                        error!(name = name.clone(), "plugin is already registered");
                        return Err(Error::DuplicateRegisteredName(name));
                    };
                    debug!(name = name.clone(), "plugin registered")
                }
                None => {
                    error!(kind = "unable to borrow mut", "InternalServerError");
                    return Err(Error::UnableToSecureLock);
                }
            };
        }
        Err(_) => {
            error!(kind = "unable to secure lock", "InternalServerError");
            return Err(Error::UnableToSecureLock);
        }
    };

    Ok(())
}
