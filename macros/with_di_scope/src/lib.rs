use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn with_di_scope(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;
    let fn_vis = &input_fn.vis;
    let fn_async = &input_fn.sig.asyncness;

    if fn_async.is_none() {
        return syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "The 'with_di_scope' macro can only be applied to async functions.",
        )
        .to_compile_error()
        .into();
    }

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_async fn #fn_name(#fn_inputs) #fn_output {
            di::DIScope::run_with_scope(|| async {
                #fn_block
            }).await
        }
    };

    expanded.into()
}
