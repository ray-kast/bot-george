#[allow(clippy::wildcard_imports)]
use super::{id::IdParts, inputs::*};
use crate::{
    attrs,
    docs::{CommandSyntax, RestArg},
    opts::FieldOpts,
    Result,
};
use anyhow::anyhow;
use proc_macro2::TokenStream;
use quote::quote_spanned;
use std::collections::HashMap;

pub struct ParseParts {
    pub fun: TokenStream,
    pub iter: Ident,
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

fn collect_rest(span: Span, opts: &FieldOpts, name: &str, iter: &Ident) -> TokenStream {
    if opts.subcommand {
        quote_spanned! { span =>
            ::docbot::Command::parse(#iter).map_err(|e| {
                ::docbot::CommandParseError::Subcommand(::std::boxed::Box::new(e))
            })
        }
    } else {
        quote_spanned! { span =>
            #iter
                .map(|s| {
                    s.as_ref().parse().map_err(|e| {
                        ::docbot::CommandParseError::BadConvert(#name, ::docbot::Anyhow::from(e))
                    })
                })
                .collect::<::std::result::Result<_, _>>()
        }
    }
}

fn field_info<'a>(
    span: Span,
    syntax: &'a CommandSyntax,
    fields: &'a Fields,
) -> Result<Vec<FieldInfo<'a>>>
{
    let args = syntax
        .required
        .iter()
        .map(|n| (FieldMode::Required, n))
        .chain(syntax.optional.iter().map(|n| (FieldMode::Optional, n)))
        .chain(match syntax.rest {
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
) -> Result<TokenStream>
{
    let info = field_info(span, &docs.syntax, fields)?;

    let args = info.into_iter().map(|FieldInfo { opts, name, mode }| {
        (
            name,
            match mode {
                FieldMode::Required => quote_spanned! { span =>
                    #iter
                        .next()
                        .ok_or(::docbot::CommandParseError::MissingRequired(#name))?
                        .as_ref()
                        .parse()
                        .map_err(|e| ::docbot::CommandParseError::BadConvert(
                            #name,
                            ::docbot::Anyhow::from(e),
                        ))?
                },
                FieldMode::Optional => quote_spanned! { span =>
                    #iter
                        .next()
                        .map(|s| s.as_ref().parse())
                        .transpose()
                        .map_err(|e| ::docbot::CommandParseError::BadConvert(
                            #name,
                            ::docbot::Anyhow::from(e),
                        ))?
                },
                FieldMode::RestRequired => {
                    let peekable = Ident::new("__peek", span);
                    let collected = collect_rest(span, &opts, name, &peekable);

                    quote_spanned! { span =>
                        {
                            let mut #peekable = #iter.peekable();

                            if let Some(..) = #peekable.peek() {
                                #collected
                            } else {
                                Err(::docbot::CommandParseError::MissingRequired(#name))
                            }
                        }?
                    }
                },
                FieldMode::RestOptional => {
                    let collected = collect_rest(span, &opts, name, iter);

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

    Ok(if let RestArg::None = docs.syntax.rest {
        let check = quote_spanned! { span =>
            if let Some(__trail) = #iter.next() {
                return Err(::docbot::CommandParseError::Trailing(__trail.as_ref().into()));
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
    let id_ty = &id_parts.ty;

    let ctors: Vec<_> = match input.commands {
        Commands::Struct(ref cmd) => {
            let ctor = ctor_fields(
                input.span,
                cmd,
                quote_spanned! { input.span => Self },
                &iter,
            )?;

            vec![quote_spanned! { input.span => #id_ty => #ctor }]
        },
        Commands::Enum(ref vars) => vars
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
                    )?;

                    Ok(quote_spanned! { *span => #id_ty::#ident => #ctor })
                },
            )
            .collect::<Result<_>>()?,
    };

    let fun = quote_spanned! { input.span =>
        let mut #iter = #iter.into_iter().fuse();

        let __id: #id_ty = match #iter.next() {
            Some(__str) => __str.as_ref().parse()?,
            None => return Err(::docbot::CommandParseError::NoInput),
        };

        Ok(match __id {
            #(#ctors),*
        })
    };

    Ok(ParseParts { fun, iter })
}
