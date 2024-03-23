use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn fiddler_registration_func(_attr: TokenStream, input: TokenStream)  -> TokenStream {
    let new_input = parse_macro_input!(input as ItemFn);
    let sig = new_input.sig.clone().ident.span().unwrap().source_text().unwrap();
    let signature: proc_macro2::TokenStream = format!("pub fn fiddler_register_plugin() -> Result<(), Error> {{\n\t{}()\n}}", sig).parse().unwrap();
    let expanded = quote! {
        #signature
        #new_input
    };
    TokenStream::from(expanded)
}

