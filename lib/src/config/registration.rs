use crate::Error;
use super::{ENV, RegisteredItem, ItemType, ConfigSpec, Callback};

pub fn register_plugin(name: String, itype: ItemType, format: ConfigSpec, creator: Callback) -> Result<(), Error> {
    let r = RegisteredItem{
        creator,
        format,
    };

    match ENV.lock() {
        Ok(mut lock) => {
            match lock.get_mut(&itype) {
                Some(i) => {
                    if let Some(_) = i.insert(name.clone(), r) {
                        return Err(Error::DuplicateRegisteredName(name))
                    }
                },
                None => return Err(Error::UnableToSecureLock),
            };
        },
        Err(_) => return Err(Error::UnableToSecureLock),
    };

    Ok(())
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::modules::processors::noop;

//     #[test]
//     fn add_input() {
//         let nope = noop::NoOp{};

//         // register_processor()
//     }
// }