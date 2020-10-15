#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![feature(bindings_after_at, proc_macro_diagnostic)]

//! Derive macro for the docbot crate

mod attrs;
mod docs;
mod opts;
mod trie;

use anyhow::anyhow;
use docs::{CommandDocs, CommandSetDocs, CommandSyntax, RestArg};
use opts::FieldOpts;
use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Literal, Span, TokenStream};
use quote::{quote_spanned, ToTokens};
use std::collections::HashMap;
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Data, DataEnum, DataStruct, DeriveInput,
    Fields, Ident,
};
use trie::Trie;

pub(crate) type Error = (anyhow::Error, Span);
pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

// TODO: Warn on missing help command

/// Construct a new command set from a string
#[proc_macro_derive(Docbot, attributes(docbot))]
pub fn derive_docbot(input: TokenStream1) -> TokenStream1 {
    let input: DeriveInput = parse_macro_input!(input);

    match derive_docbot_impl(input) {
        Ok(s) => s.into(),
        Err((e, s)) => {
            s.unwrap()
                .error(format!("Macro execution failed:\n{:?}", e))
                .emit();
            TokenStream1::new()
        },
    }
}

enum FieldMode {
    Required,
    Optional,
    RestRequired,
    RestOptional,
}

struct FieldInfo<'a> {
    mode: FieldMode,
    name: &'a str,
    opts: FieldOpts,
}

struct ConstructorData {
    next: Box<dyn Fn(Span, FieldInfo) -> TokenStream>,
    check_trailing: Box<dyn Fn(Span) -> TokenStream>,
}

struct SingleCommand {
    span: Span,
    ctor: Box<dyn Fn(&ConstructorData) -> Result<TokenStream>>,
    docs: CommandDocs,
}

struct MultiCommand {
    id: Ident,
    pat: TokenStream,
    span: Span,
    ctor: Box<dyn Fn(&ConstructorData) -> Result<TokenStream>>,
    docs: CommandDocs,
}

enum CommandSet {
    Single(SingleCommand),
    Multi(CommandSetDocs, Vec<MultiCommand>),
}

fn derive_docbot_impl(input: DeriveInput) -> Result<TokenStream> {
    let span = input.span();
    let vis = input.vis;
    let name = input.ident;
    let generics = input.generics;

    let (impl_vars, ty_vars, where_clause) = generics.split_for_impl();

    let cset = match input.data {
        Data::Struct(s) => derive_docbot_struct(&input.attrs, s, span),
        Data::Enum(e) => derive_docbot_enum(&input.attrs, e, span),
        Data::Union(_) => Err((anyhow!("cannot derive Docbot on a union."), span)),
    }
    .map_err(|(e, s)| (e.context("failed to process derive input"), s))?;

    use CommandSet::*;

    let fromstr_s = quote_spanned! { span => _s };
    let fromstr_iter = quote_spanned! { span => _iter };
    let fromstr_id = quote_spanned! { span => _id };

    fn fromstr_no_match(span: Span, fromstr_s: impl ToTokens) -> impl ToTokens {
        quote_spanned! { span => Err(::docbot::IdParseError::NoMatch(#fromstr_s.into())) }
    }

    fn fromstr_ambiguous(span: Span, fromstr_s: impl ToTokens, vals: Vec<&str>) -> impl ToTokens {
        let expected = vals.into_iter().map(|v| Literal::string(v));

        quote_spanned! { span =>
            Err(::docbot::IdParseError::Ambiguous(&[#(#expected),*], #fromstr_s.into()))
        }
    }

    let id_ty = Ident::new(&format!("{}Id", name), name.span());
    let cdata = ConstructorData {
        next: {
            let fromstr_iter = fromstr_iter.clone();
            Box::new(move |span, FieldInfo { mode, name, opts }| {
                let collect_rest = |iter| {
                    if opts.subcommand {
                        quote_spanned! { span =>
                            ::docbot::Command::parse(#iter)
                                .map_err(|e| ::docbot::CommandParseError::Subcommand(
                                    ::std::boxed::Box::new(e)
                                ))
                        }
                    } else {
                        quote_spanned! { span =>
                            #iter.map(|s| {
                                ::std::convert::TryFrom::try_from(s.as_ref()).map_err(|e| {
                                    ::docbot::CommandParseError::BadConvert(
                                        #name,
                                        ::docbot::Anyhow::from(e),
                                    )
                                })
                            }).collect::<::std::result::Result<_, _>>()
                        }
                    }
                };

                match mode {
                    FieldMode::Required => quote_spanned! { span =>
                        ::std::convert::TryFrom::try_from(
                            #fromstr_iter
                            .next()
                            .ok_or_else(|| ::docbot::CommandParseError::MissingRequired(#name))?
                            .as_ref()
                        ).map_err(|e| {
                            ::docbot::CommandParseError::BadConvert(#name, ::docbot::Anyhow::from(e))
                        })?
                    },
                    FieldMode::Optional => quote_spanned! { span =>
                        #fromstr_iter
                        .next()
                            .map(|s| ::std::convert::TryFrom::try_from(s.as_ref()))
                            .transpose()
                            .map_err(|e| {
                                ::docbot::CommandParseError::BadConvert(
                                    #name,
                                    ::docbot::Anyhow::from(e)
                                )
                            })?
                    },
                    FieldMode::RestRequired => {
                        let collected = collect_rest(quote_spanned! { span => _p });

                        quote_spanned! { span =>
                            {
                                let mut _p = #fromstr_iter.peekable();

                                if let Some(..) = _p.peek() {
                                    #collected
                                }
                                else {
                                    Err(::docbot::CommandParseError::MissingRequired(#name))
                                }
                            }?
                        }
                    },
                    FieldMode::RestOptional => {
                        let collected = collect_rest(quote_spanned! { span => #fromstr_iter });

                        quote_spanned! { span => #collected? }
                    },
                }
            })
        },
        check_trailing: {
            let fromstr_iter = fromstr_iter.clone();
            Box::new(move |span| {
                quote_spanned! { span =>
                    if let Some(s) = #fromstr_iter.next() {
                        return Err(::docbot::CommandParseError::Trailing(s.as_ref().into()));
                    }
                }
            })
        },
    };
    let id_def;
    let id_lexer;
    let parse_construct;
    let id_get;

    match cset {
        Single(SingleCommand { span, ctor, docs }) => {
            id_def = quote_spanned! { name.span() => struct #id_ty; };

            id_lexer = Trie::new(docs.syntax.ids.into_iter().map(|i| (i, &id_ty)))
                .map_err(|e| (e.context("failed to construct command lexer"), span))?
                .root()
                .to_lexer(
                    &fromstr_iter,
                    |val| quote_spanned! { span => Ok(#val) },
                    || fromstr_no_match(span, &fromstr_s),
                    |v| fromstr_ambiguous(span, &fromstr_s, v),
                );

            let ctor = ctor(&cdata)?;

            parse_construct = quote_spanned! { span => Ok(#ctor) };

            id_get = quote_spanned! { name.span() => #id_ty };
        },
        Multi(d, v) => {
            let ids = v.iter().map(|c| &c.id);

            id_def = quote_spanned! { name.span() => enum #id_ty { #(#ids),* } };

            id_lexer = Trie::new(
                v.iter()
                    .flat_map(|c| c.docs.syntax.ids.iter().map(move |i| (i, &c.id))),
            )
            .map_err(|e| (e.context("failed to construct command lexer"), span))?
            .root()
            .to_lexer(
                &fromstr_iter,
                |val| quote_spanned! { span => Ok(#id_ty::#val) },
                || fromstr_no_match(span, &fromstr_s),
                |v| fromstr_ambiguous(span, &fromstr_s, v),
            );

            let ctor_arms = v
                .iter()
                .map(|MultiCommand { span, id, ctor, .. }| {
                    let ctor = ctor(&cdata)?;

                    Ok(quote_spanned! { *span => #id_ty::#id => #ctor })
                })
                .collect::<Result<Vec<_>>>()?;

            parse_construct = quote_spanned! { span =>
                Ok(match #fromstr_id {
                    #(#ctor_arms),*
                })
            };

            let id_get_arms = v.iter().map(|MultiCommand { span, id, pat, .. }| {
                quote_spanned! { *span => #pat => #id_ty::#id }
            });

            id_get = quote_spanned! { span =>
                match self {
                    #(#id_get_arms),*
                }
            };
        },
    };

    Ok(quote_spanned! { span =>
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #vis #id_def

        impl ::std::str::FromStr for #id_ty {
            type Err = ::docbot::IdParseError;

            fn from_str(#fromstr_s: &str) -> Result<Self, Self::Err> {
                let mut #fromstr_iter = #fromstr_s.chars();

                #id_lexer
            }
        }

        impl #impl_vars ::docbot::Command for #name #ty_vars #where_clause {
            type Id = #id_ty;

            fn parse<
                I: IntoIterator<Item = S>,
                S: AsRef<str>
            >(#fromstr_iter: I) -> ::std::result::Result<Self, ::docbot::CommandParseError> {
                let mut #fromstr_iter = #fromstr_iter.into_iter().fuse();

                let #fromstr_id: #id_ty = match #fromstr_iter.next() {
                    Some(s) => s.as_ref().parse()?,
                    None => return Err(::docbot::CommandParseError::NoInput),
                };

                #parse_construct
            }

            fn id(&self) -> Self::Id { #id_get }
        }
    })
}

fn get_ident<S: AsRef<str>>(s: S, span: Span) -> Result<Ident> {
    syn::parse_str(s.as_ref()).map_err(|e| (e.into(), span))
}

fn ctor_fields(
    data: &ConstructorData,
    span: Span,
    fields: &Fields,
    syntax: &CommandSyntax,
) -> Result<(TokenStream, bool)>
{
    let ConstructorData {
        next,
        check_trailing: _,
    } = data;

    let CommandSyntax {
        ids: _,
        required,
        optional,
        rest,
    } = syntax;

    let has_rest = match rest {
        RestArg::None => false,
        _ => true,
    };

    let info = required
        .iter()
        .map(|n| (FieldMode::Required, n))
        .chain(optional.iter().map(|n| (FieldMode::Optional, n)))
        .chain(match rest {
            RestArg::None => None,
            RestArg::Optional(n) => Some((FieldMode::RestOptional, n)),
            RestArg::Required(n) => Some((FieldMode::RestRequired, n)),
        });

    let info = match fields {
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(unnamed) => info
            .zip(unnamed.unnamed.iter())
            .map(|((mode, name), field)| {
                let opts = attrs::parse_field(&field.attrs, span)?;
                Ok(FieldInfo { opts, mode, name })
            })
            .collect(),
        Fields::Named(named) => {
            let mut map: HashMap<_, _> = named
                .named
                .iter()
                .map(|f| (format!("{}", f.ident.as_ref().unwrap()), f))
                .collect();

            info.map(|(mode, name)| {
                let field = map
                    .remove(name)
                    .ok_or_else(|| (anyhow!("could not locate field {:?}", name), span))?;
                let opts = attrs::parse_field(&field.attrs, span)?;

                Ok(FieldInfo { opts, mode, name })
            })
            .collect()
        },
    }?;

    if info.len() != fields.iter().len() {
        return Err((anyhow!("mismatched number of fields and arguments"), span));
    }

    let args = info.into_iter().map(|i| (i.name, next(span, i)));

    Ok((
        match fields {
            Fields::Unit => {
                if required.len() > 0 || optional.len() > 0 || has_rest {
                    span.unwrap()
                        .error("unit types cannot have command arguments")
                        .emit();
                }

                TokenStream::new()
            },
            Fields::Unnamed(..) => {
                let args = args.map(|(_, a)| a);

                quote_spanned! { span => ( #(#args),* ) }
            },
            Fields::Named(..) => {
                let args = args
                    .map(|(name, arg)| {
                        let id = get_ident(name, span)?;
                        Ok(quote_spanned! { span => #id: #arg })
                    })
                    .collect::<Result<Vec<_>>>()?;

                quote_spanned! { span => { #(#args),* } }
            },
        },
        !has_rest,
    ))
}

fn check_trailing(
    data: &ConstructorData,
    span: Span,
    val: TokenStream,
    iter_available: bool,
) -> TokenStream
{
    if iter_available {
        let check = (data.check_trailing)(span);
        quote_spanned! { span =>
            {
                let _val = #val;
                #check;
                _val
            }
        }
    } else {
        val
    }
}

fn derive_docbot_struct(
    attrs: &Vec<Attribute>,
    data: DataStruct,
    span: Span,
) -> Result<CommandSet>
{
    let docs: CommandDocs = attrs::parse_outer(attrs, span)?;
    let fields = data.fields;

    let doc_span = docs.span;
    let syntax = docs.syntax.clone();

    Ok(CommandSet::Single(SingleCommand {
        span,
        ctor: Box::new(move |data| {
            let (fields, iter_available) = ctor_fields(data, doc_span, &fields, &syntax)?;

            Ok(check_trailing(
                data,
                span,
                quote_spanned! { span => Self #fields },
                iter_available,
            ))
        }),
        docs,
    }))
}

fn derive_docbot_enum(attrs: &Vec<Attribute>, data: DataEnum, span: Span) -> Result<CommandSet> {
    let outer = attrs::parse_outer(attrs, span)?;

    let vars = data
        .variants
        .into_iter()
        .map(|v| {
            let span = v.span();
            let id = v.ident.clone();
            let fields = v.fields;

            let docs: CommandDocs = attrs::parse_variant(&v.attrs, span)?;

            let doc_span = docs.span;
            let syntax = docs.syntax.clone();

            Ok(MultiCommand {
                id: v.ident.clone(),
                pat: match fields {
                    Fields::Named(..) => quote_spanned! { span => Self::#id { .. } },
                    Fields::Unnamed(..) => quote_spanned! { span => Self::#id(..) },
                    Fields::Unit => quote_spanned! { span => Self::#id },
                },
                span,
                ctor: Box::new(move |cdata| {
                    let (fields, iter_available) = ctor_fields(cdata, doc_span, &fields, &syntax)?;

                    Ok(check_trailing(
                        cdata,
                        span,
                        quote_spanned! { span => Self::#id #fields },
                        iter_available,
                    ))
                }),
                docs,
            })
        })
        .collect::<Result<_>>()?;

    Ok(CommandSet::Multi(outer, vars))
}
