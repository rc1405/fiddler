use crate::Error;
pub mod file;
pub mod stdin;

pub(crate) fn register_plugins() -> Result<(), Error> {
    file::register_file()?;
    stdin::register_stdin()?;
    Ok(())
}
