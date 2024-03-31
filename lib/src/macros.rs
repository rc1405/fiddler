macro_rules! include_plugins {
    ($package: tt) => {
        include!(concat!(env!("OUT_DIR"), concat!("/", $package, "/mod.rs")));
    };
}

pub(crate) use include_plugins;
