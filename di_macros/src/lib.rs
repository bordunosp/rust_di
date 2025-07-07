extern crate proc_macro;

mod register_macros;
mod with_di_scope;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn with_di_scope(_attr: TokenStream, item: TokenStream) -> TokenStream {
    with_di_scope::with_di_scope(item)
}

#[proc_macro_attribute]
pub fn registry(attr: TokenStream, item: TokenStream) -> TokenStream {
    register_macros::generate_di_macro(attr, item)
}
