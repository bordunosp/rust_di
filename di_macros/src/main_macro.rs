use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{ItemFn, parse_macro_input};

pub fn expand_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Розбираємо переданий fn
    let input_fn = parse_macro_input!(item as ItemFn);
    let attrs = &input_fn.attrs;
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let block = &input_fn.block;

    // Якщо fn не async — видаємо помилку
    if sig.asyncness.is_none() {
        let span = attrs
            .iter()
            .find(|a| a.path().is_ident("rust_di") || a.path().is_ident("main"))
            .map(|a| a.span())
            .unwrap_or(sig.ident.span());

        let compile_err = syn::Error::new(
            span,
            "`#[rust_di::main]` must be applied to an `async fn` and placed above `#[tokio::main]`.",
        )
            .to_compile_error();

        let original_fn = input_fn.to_token_stream();

        return quote! {
            #compile_err
            #original_fn
        }
        .into();
    }

    // Для async fn — просто вставляємо DI-кроки перед існуючими await
    let expanded = quote! {
        #(#attrs)*
        #vis #sig {
            rust_di::initialize().await;
            rust_di::DIScope::run_with_scope(|| async #block).await;
        }
    };

    expanded.into()
}
