use crate::Error;
pub mod lines;
pub mod noop;
#[cfg(feature = "python")]
pub mod python;
pub mod switch;

pub fn register_plugins() -> Result<(), Error> {
    lines::register_lines()?;
    noop::register_noop()?;
    #[cfg(feature = "python")]
    python::register_python()?;
    switch::register_switch()?;
    Ok(())
}