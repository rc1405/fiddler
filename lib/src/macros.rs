
macro_rules! include_plugins {
    ($package: tt) => {
        include!(concat!(env!("OUT_DIR"), concat!("/", $package, "/mod.rs")));
    };
}

pub(crate) use include_plugins;

// #[proc_macro_attribute]
// pub fn register_plugin(item: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(item);
//     // fn REGISTER_PLUGIN() -> Result<(), Error> {
//     // }
// }