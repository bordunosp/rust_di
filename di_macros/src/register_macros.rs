use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::hash::{Hash, Hasher};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Expr, ItemImpl, Lit, Meta, Path, Token, parse_macro_input};

#[derive(Hash)]
pub(crate) enum DiKind {
    Singleton,
    Scoped,
    Transient,
}

pub(crate) struct DiRegistration {
    pub kind: DiKind,
    pub use_factory: bool,
    pub factory_path: Option<Path>,
    pub name: Option<String>,
}

pub(crate) fn generate_di_macro(attr: TokenStream, item: TokenStream) -> TokenStream {
    let registrations = parse_registry_args(attr);
    let input = parse_macro_input!(item as ItemImpl);
    let self_ty = &input.self_ty;

    let mut ctors = Vec::new();

    for reg in registrations {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let type_name = quote!(#self_ty).to_string();
        type_name.hash(&mut hasher);
        reg.name.hash(&mut hasher);
        reg.use_factory.hash(&mut hasher);
        reg.kind.hash(&mut hasher);
        let hash = hasher.finish();

        let fn_name = format_ident!(
            "__register_di_{}_{}_{}",
            match reg.kind {
                DiKind::Singleton => "singleton",
                DiKind::Scoped => "scoped",
                DiKind::Transient => "transient",
            },
            type_name
                .replace("::", "_")
                .replace('<', "_")
                .replace('>', "_"),
            hash
        );

        let name_literal = syn::LitStr::new(
            reg.name.as_deref().unwrap_or(""),
            proc_macro2::Span::call_site(),
        );

        let registration = match reg.kind {
            DiKind::Singleton => {
                if reg.use_factory {
                    if let Some(factory_path) = &reg.factory_path {
                        quote! {
                            ::di::register_singleton_name::<#self_ty, _, _>(#name_literal, |scope| async move {
                                #factory_path::create(scope).await
                            }).await
                        }
                    } else {
                        quote! {
                            ::di::register_singleton_name::<#self_ty, _, _>(#name_literal, |scope| async move {
                                <#self_ty as ::di::DiFactory>::create(scope).await
                            }).await
                        }
                    }
                } else {
                    quote! {
                        ::di::register_singleton_name::<#self_ty, _, _>(#name_literal, |_scope| async move {
                            Ok(<#self_ty as ::std::default::Default>::default())
                        }).await
                    }
                }
            }
            DiKind::Scoped => {
                let factory = if reg.use_factory {
                    if let Some(factory_path) = &reg.factory_path {
                        quote! {
                            let instance = #factory_path::create(scope).await
                                .map_err(|e| ::di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    } else {
                        quote! {
                            let instance = <#self_ty as ::di::DiFactory>::create(scope).await
                                .map_err(|e| ::di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    }
                } else {
                    quote! {
                        Ok(<#self_ty as ::std::default::Default>::default())
                    }
                };

                quote! {
                    ::di::register_scope_name::<#self_ty, _, _>(#name_literal, ::std::sync::Arc::new(|scope| {
                        Box::pin(async move {
                            #factory
                        })
                    })).await
                }
            }
            DiKind::Transient => {
                let factory = if reg.use_factory {
                    if let Some(factory_path) = &reg.factory_path {
                        quote! {
                            let instance = #factory_path::create(scope).await
                                .map_err(|e| ::di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    } else {
                        quote! {
                            let instance = <#self_ty as ::di::DiFactory>::create(scope).await
                                .map_err(|e| ::di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    }
                } else {
                    quote! {
                        Ok(<#self_ty as ::std::default::Default>::default())
                    }
                };

                quote! {
                    ::di::register_transient_name::<#self_ty, _, _>(#name_literal, ::std::sync::Arc::new(|scope| {
                        Box::pin(async move {
                            #factory
                        })
                    })).await
                }
            }
        };

        ctors.push(quote! {
            #[doc(hidden)]
            #[::ctor::ctor]
            fn #fn_name() {
                let _ = ::tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async {
                        let scope = ::di::DIScope::new().await;
                        #registration
                    });
            }
        });
    }

    let expanded = quote! {
        #input
        #(#ctors)*
    };

    TokenStream::from(expanded)
}

fn parse_registry_args(attr: TokenStream) -> Vec<DiRegistration> {
    let metas = Punctuated::<Meta, Token![,]>::parse_terminated
        .parse(attr.into())
        .expect("Failed to parse registry attribute");

    let mut registrations = Vec::new();

    for meta in metas {
        match meta {
            Meta::Path(path) => {
                if let Some(ident) = path.get_ident() {
                    let kind = match ident.to_string().as_str() {
                        "Singleton" => DiKind::Singleton,
                        "Scoped" => DiKind::Scoped,
                        "Transient" => DiKind::Transient,
                        _ => continue,
                    };

                    registrations.push(DiRegistration {
                        kind,
                        use_factory: false,
                        factory_path: None,
                        name: None,
                    });
                }
            }
            Meta::List(list) => {
                let kind = match list.path.get_ident() {
                    Some(ident) if ident == "Singleton" => DiKind::Singleton,
                    Some(ident) if ident == "Scoped" => DiKind::Scoped,
                    Some(ident) if ident == "Transient" => DiKind::Transient,
                    _ => continue,
                };

                let mut use_factory = false;
                let mut factory_path = None;
                let mut name = None;

                let nested = Punctuated::<Meta, Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .expect("Failed to parse nested meta");

                for meta in nested {
                    match meta {
                        Meta::Path(p) if p.is_ident("factory") => {
                            use_factory = true;
                            factory_path = None;
                        }
                        Meta::NameValue(nv) if nv.path.is_ident("factory") => {
                            use_factory = true;

                            if let Expr::Path(expr_path) = nv.value {
                                factory_path = Some(expr_path.path);
                            } else {
                                panic!(
                                    "Expected a path for `factory = ...`, like `factory = MyFactory`"
                                );
                            }
                        }
                        Meta::NameValue(nv) if nv.path.is_ident("name") => {
                            if let Expr::Lit(expr_lit) = nv.value {
                                if let Lit::Str(lit_str) = expr_lit.lit {
                                    name = Some(lit_str.value());
                                }
                            }
                        }
                        _ => {}
                    }
                }

                registrations.push(DiRegistration {
                    kind,
                    use_factory,
                    factory_path,
                    name,
                });
            }
            _ => {}
        }
    }

    registrations
}
