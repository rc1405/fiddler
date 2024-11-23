use crate::Error;
pub mod drop;
#[cfg(feature = "elasticsearch")]
pub mod elasticsearch;
pub mod stdout;
pub mod switch;

pub fn register_plugins() -> Result<(), Error> {
    drop::register_drop()?;
    #[cfg(feature = "elasticsearch")]
    elasticsearch::register_elasticsearch()?;
    stdout::register_stdout()?;
    switch::register_switch()?;
    Ok(())
}