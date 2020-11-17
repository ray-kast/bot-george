#[allow(clippy::wildcard_imports)]
use super::{id::IdParts, inputs::*};
use crate::{attrs, opts::FieldOpts, Result};
use anyhow::anyhow;
use proc_macro2::TokenStream;
use quote::quote_spanned;
use std::collections::HashMap;

pub struct ParseParts {
    pub items: TokenStream,
}

enum FieldMode {
    Required,
    Optional,
    RestRequired,
    RestOptional,
}

struct FieldInfo<'a> {
    opts: FieldOpts,
    name: &'a str,
    mode: FieldMode,
}

fn collect_rest(span: Span, opts: &FieldOpts, name: &str, iter: &Ident, id: &Ident) -> TokenStream {
    if opts.subcommand {
        quote_spanned! { span =>
            ::docbot::Command::parse(#iter).map_err(|e| ::docbot::CommandParseError::Subcommand(
                ::docbot::CommandId::to_str(&#id),
                ::std::boxed::Box::new(e),
            ))
        }
    } else {
        quote_spanned! { span =>
            #iter
                .map(|s| {
                    s.as_ref().parse().map_err(|e| {
                        ::docbot::CommandParseError::BadConvert(
                            ::docbot::ArgumentName {
                                cmd: ::docbot::CommandId::to_str(&#id),
                                arg: #name,
                            },
                            ::docbot::Anyhow::from(e)
                        )
                    })
                })
                .collect::<::std::result::Result<_, _>>()
        }
    }
}

fn field_info<'a>(
    span: Span,
    usage: &'a CommandUsage,
    fields: &'a Fields,
) -> Result<Vec<FieldInfo<'a>>>
{
    let args = usage
        .required
        .iter()
        .map(|n| (FieldMode::Required, n))
        .chain(usage.optional.iter().map(|n| (FieldMode::Optional, n)))
        .chain(match usage.rest {
            RestArg::None => None,
            RestArg::Optional(ref n) => Some((FieldMode::RestOptional, n)),
            RestArg::Required(ref n) => Some((FieldMode::RestRequired, n)),
        });

    let args = match fields {
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(u) => args
            .zip(u.unnamed.iter())
            .map(|((mode, name), field)| {
                Ok(FieldInfo {
                    opts: attrs::parse_field(&field.attrs, span)?,
                    mode,
                    name,
                })
            })
            .collect(),
        Fields::Named(n) => {
            let mut map: HashMap<_, _> = n
                .named
                .iter()
                .map(|f| (format!("{}", f.ident.as_ref().unwrap()), f))
                .collect();

            args.map(|(mode, name)| {
                Ok(FieldInfo {
                    opts: attrs::parse_field(
                        &map.remove(name)
                            .ok_or_else(|| (anyhow!("could not locate field {:?}", name), span))?
                            .attrs,
                        span,
                    )?,
                    mode,
                    name,
                })
            })
            .collect::<Result<_>>()
        },
    }?;

    if args.len() != fields.iter().len() {
        return Err((anyhow!("mismatched number of fields and arguments"), span));
    }

    Ok(args)
}

fn ctor_fields(
    span: Span,
    Command { docs, fields }: &Command,
    path: TokenStream,
    iter: &Ident,
    id: &Ident,
) -> Result<TokenStream>
{
    let info = field_info(span, &docs.usage, fields)?;

    let args = info.into_iter().map(|FieldInfo { opts, name, mode }| {
        (
            name,
            match mode {
                FieldMode::Required => quote_spanned! { span =>
                    #[allow(clippy::or_fun_call)]
                    #iter
                        .next()
                        .ok_or(::docbot::CommandParseError::MissingRequired(
                            ::docbot::ArgumentName {
                                cmd: ::docbot::CommandId::to_str(&#id),
                                arg: #name
                            }
                        ))?
                        .as_ref()
                        .parse()
                        .map_err(|e| ::docbot::CommandParseError::BadConvert(
                            ::docbot::ArgumentName {
                                cmd: ::docbot::CommandId::to_str(&#id),
                                arg: #name,
                            },
                            ::docbot::Anyhow::from(e),
                        ))?
                },
                FieldMode::Optional => quote_spanned! { span =>
                    #iter
                        .next()
                        .map(|s| s.as_ref().parse())
                        .transpose()
                        .map_err(|e| ::docbot::CommandParseError::BadConvert(
                            ::docbot::ArgumentName {
                                cmd: ::docbot::CommandId::to_str(&#id),
                                arg: #name,
                            },
                            ::docbot::Anyhow::from(e),
                        ))?
                },
                FieldMode::RestRequired => {
                    let peekable = Ident::new("__peek", span);
                    let collected = collect_rest(span, &opts, name, &peekable, id);

                    quote_spanned! { span =>
                        {
                            let mut #peekable = #iter.peekable();

                            if let Some(..) = #peekable.peek() {
                                #collected
                            } else {
                                Err(::docbot::CommandParseError::MissingRequired(
                                    ::docbot::ArgumentName {
                                        cmd: ::docbot::CommandId::to_str(&#id),
                                        arg: #name,
                                    }
                                ))
                            }
                        }?
                    }
                },
                FieldMode::RestOptional => {
                    let collected = collect_rest(span, &opts, name, iter, id);

                    quote_spanned! { span => #collected? }
                },
            },
        )
    });

    let ret = match fields {
        Fields::Unit => path,
        Fields::Unnamed(..) => {
            let args = args.map(|(_, a)| a);

            quote_spanned! { span => #path (#(#args),*) }
        },
        Fields::Named(..) => {
            let args = args
                .map(|(name, arg)| {
                    let id: Ident = syn::parse_str(name).map_err(|e| (e.into(), span))?;
                    Ok(quote_spanned! { span => #id: #arg })
                })
                .collect::<Result<Vec<_>>>()?;

            quote_spanned! { span => #path { #(#args),* } }
        },
    };

    Ok(if let RestArg::None = docs.usage.rest {
        let check = quote_spanned! { span =>
            if let Some(__trail) = #iter.next() {
                return Err(::docbot::CommandParseError::Trailing(
                    ::docbot::CommandId::to_str(&#id),
                    __trail.as_ref().into(),
                ));
            }
        };

        quote_spanned! { span =>
            {
                let __rest = #ret;
                #check;
                __rest
            }
        }
    } else {
        ret
    })
}

pub fn emit(input: &InputData, id_parts: &IdParts) -> Result<ParseParts> {
    let iter = Ident::new("__iter", input.span);
    let id = Ident::new("__id", input.span);
    let id_ty = &id_parts.ty;

    let ctors: Vec<_> = match input.commands {
        Commands::Struct(_, ref cmd) => {
            let ctor = ctor_fields(
                input.span,
                cmd,
                quote_spanned! { input.span => Self },
                &iter,
                &id,
            )?;

            vec![quote_spanned! { input.span => #id_ty => #ctor }]
        },
        Commands::Enum(_, ref vars) => vars
            .iter()
            .map(
                |CommandVariant {
                     span,
                     ident,
                     command,
                     ..
                 }| {
                    let ctor = ctor_fields(
                        *span,
                        command,
                        quote_spanned! { *span => Self::#ident },
                        &iter,
                        &id,
                    )?;

                    Ok(quote_spanned! { *span => #id_ty::#ident => #ctor })
                },
            )
            .collect::<Result<_>>()?,
    };

    // Quote variables
    let name = input.ty;
    let (impl_vars, ty_vars, where_clause) = input.generics.split_for_impl();
    let id_get_fn = &id_parts.get_fn;

    let items = quote_spanned! { input.span =>
        impl #impl_vars ::docbot::Command for #name #ty_vars #where_clause {
            type Id = #id_ty;

            fn parse<
                I: IntoIterator<Item = S>,
                S: AsRef<str>,
            >(#iter: I) -> ::std::result::Result<Self, ::docbot::CommandParseError> {
                let mut #iter = #iter.into_iter().fuse();

                let #id: #id_ty = match #iter.next() {
                    Some(__str) => __str.as_ref().parse()?,
                    None => return Err(::docbot::CommandParseError::NoInput),
                };

                Ok(match #id {
                    #(#ctors),*
                })
            }

            fn id(&self) -> Self::Id { #id_get_fn }
        }
    };

    Ok(ParseParts { items })
}
