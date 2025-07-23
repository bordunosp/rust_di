use proc_macro::TokenStream;
use quote::quote;
use std::hash::Hash;
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

    let mut submissions = Vec::new();

    for reg in registrations {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let type_name = quote!(#self_ty).to_string();
        type_name.hash(&mut hasher);
        reg.name.hash(&mut hasher);
        reg.use_factory.hash(&mut hasher);
        reg.kind.hash(&mut hasher);

        let name_literal = syn::LitStr::new(
            reg.name.as_deref().unwrap_or(""),
            proc_macro2::Span::call_site(),
        );

        let registration = match reg.kind {
            DiKind::Singleton => {
                if reg.use_factory {
                    if let Some(factory_path) = &reg.factory_path {
                        quote! {
                            ::rust_di::core::registry::register_singleton_name::<#self_ty, _, _>(#name_literal, |scope| async move {
                                #factory_path::create(scope).await
                            }).await
                        }
                    } else {
                        quote! {
                            ::rust_di::core::registry::register_singleton_name::<#self_ty, _, _>(#name_literal, |scope| async move {
                                <#self_ty as ::rust_di::core::factory::DiFactory>::create(scope).await
                            }).await
                        }
                    }
                } else {
                    quote! {
                        ::rust_di::core::registry::register_singleton_name::<#self_ty, _, _>(#name_literal, |_scope| async move {
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
                                .map_err(|e| ::rust_di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    } else {
                        quote! {
                            let instance = <#self_ty as ::rust_di::core::factory::DiFactory>::create(scope).await
                                .map_err(|e| ::rust_di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    }
                } else {
                    quote! {
                        Ok(<#self_ty as ::std::default::Default>::default())
                    }
                };

                quote! {
                    ::rust_di::core::registry::register_scope_name::<#self_ty, _, _>(
                        #name_literal,
                        |scope| Box::pin(async move { #factory })
                    ).await
                }
            }
            DiKind::Transient => {
                let factory = if reg.use_factory {
                    if let Some(factory_path) = &reg.factory_path {
                        quote! {
                            let instance = #factory_path::create(scope).await
                                .map_err(|e| ::rust_di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    } else {
                        quote! {
                            let instance = <#self_ty as ::rust_di::core::factory::DiFactory>::create(scope).await
                                .map_err(|e| ::rust_di::DiError::FactoryError(Box::new(e)))?;
                            Ok(instance)
                        }
                    }
                } else {
                    quote! {
                        Ok(<#self_ty as ::std::default::Default>::default())
                    }
                };

                quote! {
                    ::rust_di::core::registry::register_transient_name::<#self_ty, _, _>(
                        #name_literal,
                        |scope| Box::pin(async move { #factory })
                    ).await
                }
            }
        };

        submissions.push(quote! {
            ::rust_di::inventory::submit! {
                ::rust_di::core::di_inventory::DiConstructor {
                    init: || Box::pin(async move {
                        let scope = ::rust_di::DIScope::new().await;
                        let _ = #registration;
                    })
                }
            }
        });
    }

    let expanded = quote! {
        #input
        #(#submissions)*
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
