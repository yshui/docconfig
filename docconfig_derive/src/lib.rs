extern crate proc_macro;
use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::quote;
use serde_derive_internals::{
    ast::{self, Data},
    Ctxt, Derive,
};
use syn::{parse_macro_input, DeriveInput};
use syn::{Attribute, Lit::Str, Meta::NameValue, MetaNameValue};

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Unsupported serde attribute")]
    SerdeAttr,
    #[error("Generic type is not supported")]
    Generics,
    #[error("Unsupported member type")]
    MemberType,
    #[error("Unnamed member not supported")]
    Unnamed,
}

fn compile_error(errors: Vec<syn::Error>) -> proc_macro2::TokenStream {
    let compile_errors = errors.iter().map(syn::Error::to_compile_error);
    quote::quote! {
        #(#compile_errors)*
    }
}

fn get_doc(attrs: &[Attribute]) -> Vec<String> {
    let attrs = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path.is_ident("doc") {
                return None;
            }

            let meta = attr.parse_meta().ok()?;
            if let NameValue(MetaNameValue { lit: Str(s), .. }) = meta {
                return Some(s.value());
            }

            None
        })
        .collect::<Vec<_>>();

    let mut lines = attrs
        .iter()
        .flat_map(|a| a.split('\n'))
        .map(str::trim)
        .skip_while(|s| s.is_empty())
        .collect::<Vec<_>>();

    if let Some(&"") = lines.last() {
        lines.pop();
    }

    // Added for backward-compatibility, but perhaps we shouldn't do this
    // https://github.com/rust-lang/rust/issues/32088
    if lines.iter().all(|l| l.starts_with('*')) {
        for line in lines.iter_mut() {
            *line = line[1..].trim()
        }
        while let Some(&"") = lines.first() {
            lines.remove(0);
        }
    };

    lines.into_iter().map(ToOwned::to_owned).collect()
}

fn expand_members<'a>(
    writer_var: syn::Ident,
    f: impl Iterator<Item = &'a serde_derive_internals::ast::Field<'a>>,
    cb: impl Fn(
        &'a serde_derive_internals::ast::Field<'a>,
    ) -> Result<proc_macro2::TokenStream, Vec<syn::Error>>,
) -> Result<proc_macro2::TokenStream, Vec<syn::Error>> {
    let mut body = proc_macro2::TokenStream::new();
    for field in f {
        let doc = get_doc(&field.original.attrs);
        let ident = if let syn::Member::Named(ident) = &field.member {
            ident
        } else {
            return Err(vec![syn::Error::new_spanned(
                field.original,
                Error::Unnamed,
            )]);
        };
        for docline in doc {
            body.extend(quote! {
                writeln!(#writer_var, "## {}", #docline)?;
            })
        }
        body.extend(cb(field)?);
        body.extend(quote! {
            path.push(stringify!(#ident));
        });
        match field.ty {
            syn::Type::Path(ty) => {
                use serde_derive_internals::attr::Default;
                body.extend(quote! {
                    if !<#ty as DocConfig>::is_plain() {
                        writeln!(#writer_var, "[{}]", path.join("."));
                    }
                });

                let with_default = match field.attrs.default() {
                    Default::None => {
                        quote! {
                            <#ty as DocConfig>::write_doc_config(#writer_var, &path, None)?;
                        }
                    }
                    Default::Default => {
                        quote! {
                            <#ty as DocConfig>::write_doc_config(#writer_var, &path, Some(&Default::default()))?;
                        }
                    }
                    Default::Path(p) => {
                        quote! {
                            <#ty as DocConfig>::write_doc_config(#writer_var, &path, Some(&#p()))?;
                        }
                    }
                };
                body.extend(quote! {
                            if let Some(def) = def {
                                    <#ty as DocConfig>::write_doc_config(#writer_var, &path, Some(&def.#ident))?;
                            } else {
                                #with_default
                            }
                        });
            }
            _ => {
                return Err(vec![syn::Error::new_spanned(
                    field.original,
                    Error::MemberType,
                )])
            }
        }
        body.extend(quote! {
            path.pop();
        });
    }
    Ok(body)
}

fn expand_untagged_enum(
    v: Vec<serde_derive_internals::ast::Variant>,
    tag: Option<&str>,
    nested: bool,
) -> Result<proc_macro2::TokenStream, Vec<syn::Error>> {
    // Collect fields of all variants
    let fields: Result<HashMap<_, _>, _> = v
        .iter()
        .map(|v| v.fields.iter().map(|f| (f, &*v)))
        .flatten()
        .map(|(f, v)| match &f.member {
            syn::Member::Named(ident) => Ok((ident.to_string(), (f, v))),
            syn::Member::Unnamed(_) => {
                Err(vec![syn::Error::new_spanned(&f.original, Error::Unnamed)])
            }
        })
        .collect();
    let fields = fields?;
    expand_members(
        syn::Ident::new("w", proc_macro2::Span::call_site()),
        fields.values().map(|(f, _)| *f),
        |f| {
            if let Some(tag) = tag {
                if let syn::Member::Named(ident) = &f.member {
                    let (_, variant) = fields.get(&ident.to_string()).unwrap();
                    let variant_name = variant.attrs.name().deserialize_name();
                    if nested {
                        Ok(
                            quote! { writeln!(w, "## only meaningful if {} == \"{}\" in the upper level", stringify!(#tag), stringify!(#variant_name)) },
                        )
                    } else {
                        Ok(
                            quote! { writeln!(w, "## only meaningful if {} == \"{}\"", stringify!(#tag), stringify!(#variant_name)) },
                        )
                    }
                } else {
                    panic!();
                }
            } else {
                Ok(quote! {})
            }
        },
    )
}

fn expand(input: DeriveInput) -> Result<proc_macro2::TokenStream, Vec<syn::Error>> {
    let ctx = Ctxt::new();
    // Looking for top level serde attributes
    let ast = ast::Container::from_ast(&ctx, &input, Derive::Deserialize);
    ctx.check()?;
    let ast = ast.unwrap();
    let doc = get_doc(&input.attrs);

    if ast.generics.params.len() > 0 {
        return Err(vec![syn::Error::new_spanned(ast.generics, Error::Generics)]);
    }

    let mut body = proc_macro2::TokenStream::new();
    body.extend(
        doc.into_iter()
            .map(|d| quote! { writeln!(w, "## {}", #d)?; }),
    );
    match ast.data {
        Data::Enum(e) => {
            // do we have a tag?
            use serde_derive_internals::attr::TagType;
            body.extend(match ast.attrs.tag() {
                TagType::None => expand_untagged_enum(e, None, false)?,
                TagType::Internal { tag } => {
                    let mut header = quote! {};
                    header.extend(expand_untagged_enum(e, Some(tag.as_str()), false)?);
                    header
                }
                TagType::Adjacent { tag, content } => {
                    let mut header = quote! {};
                    header.extend(expand_untagged_enum(e, Some(tag.as_str()), true)?);
                    header
                }
                TagType::External => e
                    .iter()
                    .map(|v| {
                        let doc = get_doc(&v.original.attrs);
                        let ident = &v.ident;
                        let doc: proc_macro2::TokenStream = doc
                            .into_iter()
                            .map(|d| quote! { writeln!(w, "## {}", #d)?; })
                            .collect();
                        quote! {
                            #doc
                            path.push(stringify!(#ident));
                            writeln!(w, "## one and only one of the following can be set")?;
                            writeln!(w, "[{}]", path.join("."))?;
                        }
                    })
                    .collect(),
            });
        }
        Data::Struct(_, s) => {
            body.extend(expand_members(
                syn::Ident::new("w", proc_macro2::Span::call_site()),
                s.iter(),
                |_| Ok(quote! {}),
            )?);
        }
    }

    let ident = ast.ident;
    Ok(quote::quote! {
        #[automatically_derived]
        impl docconfig::DocConfig for #ident {
            type Error = std::io::Error;
            fn is_plain() -> bool {
                false
            }
            fn write_doc_config(mut w: &mut impl std::io::Write, path: &[&str], def: Option<&Self>) -> Result<(), Self::Error> {
                use docconfig::DocConfig;
                let mut path: Vec<&str> = path.iter().map(|s| *s).collect();
                #body
                Ok(())
            }
        }
    })
}
#[proc_macro_derive(DocConfig)]
pub fn derive_docconfig(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    expand(input).unwrap_or_else(compile_error).into()
}
