//! Helper macro for developing fiddler modules
//! 
//! Fiddler [`fiddler::config::Callback`] requires a return signature of `std::pin::Pin<Box<dyn core::future::Future<Output = Result<ExecutionType, Error>> + Send>>`
//! This helper macro will accept a function signature of `Fn(conf: Value) -> Result<ExecutionType, Error>`and convert it
//! to the async Bos::pin future that is needed to store as a type
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_str, ItemFn, ReturnType};

#[proc_macro_attribute]
pub fn fiddler_registration_func(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);
    let func_name = func.sig.ident.clone();
    assert!(func.sig.asyncness.is_none(), "async not supported");
    let func_starter = func.sig.clone().fn_token;
    let inputs = func.sig.clone().inputs;
    let return_type = parse_str::<ReturnType>("-> std::pin::Pin<Box<dyn core::future::Future<Output = Result<ExecutionType, Error>> + Send>>").unwrap();

    let mut statements = proc_macro2::TokenStream::new();
    for i in func.block.stmts {
        statements.extend(i.to_token_stream());
    }

    quote! {
        #func_starter
        #func_name(#inputs) #return_type  {
            Box::pin(async move {
                #statements
            })
        }
    }
    .to_token_stream()
    .into()
}
