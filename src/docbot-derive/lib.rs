#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![feature(bindings_after_at, proc_macro_diagnostic)]

//! Derive macro for the docbot crate

mod docs;
mod trie;

use anyhow::{anyhow, Context};
use docs::{CommandDocs, CommandSetDocs};
use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Literal, Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
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

struct MultiCommand {
    id: Ident,
    pat: TokenStream,
    docs: CommandDocs,
}

enum CommandSet {
    Single(CommandDocs),
    Multi(CommandSetDocs, Vec<MultiCommand>),
}

fn derive_docbot_impl(input: DeriveInput) -> Result<TokenStream> {
    let span = input.span();
    let vis = input.vis;
    let name = input.ident;
    let generics = input.generics;

    let (impl_vars, ty_vars, where_clause) = generics.split_for_impl();

    let cset = match input.data {
        Data::Struct(s) => derive_docbot_struct(input.attrs, s, span),
        Data::Enum(e) => derive_docbot_enum(input.attrs, e, span),
        Data::Union(_) => Err((anyhow!("cannot derive Docbot on a union."), span)),
    }
    .map_err(|(e, s)| (e.context("failed to process derive input"), s))?;

    use CommandSet::*;

    let fromstr_s = quote! { s };
    let fromstr_iter = quote! { iter };

    fn fromstr_no_match(fromstr_s: impl ToTokens) -> impl ToTokens {
        quote! { Err(::docbot::IdParseError::NoMatch(#fromstr_s.into())) }
    }

    fn fromstr_ambiguous(fromstr_s: impl ToTokens, vals: Vec<&str>) -> impl ToTokens {
        let expected = vals.into_iter().map(|v| Literal::string(v));

        quote! { Err(::docbot::IdParseError::Ambiguous(&[#(#expected),*], #fromstr_s.into())) }
    }

    let id_ty = Ident::new(&format!("{}Id", name), name.span());
    let id_def;
    let id_lexer;
    let parse_construct;
    let id_get;

    match cset {
        Single(c) => {
            id_def = quote! { struct #id_ty; };

            id_lexer = Trie::new(c.ids.into_iter().map(|i| (i, &id_ty)))
                .map_err(|e| (e.context("failed to construct command lexer"), span))?
                .root()
                .to_lexer(
                    &fromstr_iter,
                    |val| quote! { Ok(#val) },
                    || fromstr_no_match(&fromstr_s),
                    |v| fromstr_ambiguous(&fromstr_s, v),
                );

            parse_construct = quote! { todo!() };

            id_get = quote! { #id_ty };
        },
        Multi(d, v) => {
            let ids = v.iter().map(|c| &c.id);

            id_def = quote! { enum #id_ty { #(#ids),* } };

            id_lexer = Trie::new(
                v.iter()
                    .flat_map(|c| c.docs.ids.iter().map(move |i| (i, &c.id))),
            )
            .map_err(|e| (e.context("failed to construct command lexer"), span))?
            .root()
            .to_lexer(
                &fromstr_iter,
                |val| quote! { Ok(#id_ty::#val) },
                || fromstr_no_match(&fromstr_s),
                |v| fromstr_ambiguous(&fromstr_s, v),
            );

            parse_construct = quote! { todo!() };

            let id_get_arms = v.iter().map(|MultiCommand { id, pat, .. }| {
                quote! { #pat => #id_ty::#id }
            });

            id_get = quote! {
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
            >(iter: I) -> Result<Self, ::docbot::CommandParseError> {
                let mut iter = iter.into_iter();

                let id: #id_ty = match iter.next() {
                    Some(s) => s.as_ref().parse()?,
                    None => return Err(::docbot::CommandParseError::NoInput),
                };

                #parse_construct
            }

            fn id(&self) -> Self::Id { #id_get }
        }
    })
}

fn derive_docbot_struct(
    attrs: Vec<Attribute>,
    _data: DataStruct,
    span: Span,
) -> Result<CommandSet>
{
    let outer = docs::parse_outer(attrs, span)?;

    Ok(CommandSet::Single(outer))
}

fn derive_docbot_enum(attrs: Vec<Attribute>, data: DataEnum, span: Span) -> Result<CommandSet> {
    let outer = docs::parse_outer(attrs, span)?;

    let vars = data
        .variants
        .into_iter()
        .map(|v| {
            let span = v.span();

            let id = &v.ident;

            Ok(MultiCommand {
                id: v.ident.clone(),
                pat: match v.fields {
                    Fields::Named(..) => quote! { Self::#id { .. } },
                    Fields::Unnamed(..) => quote! { Self::#id(..) },
                    Fields::Unit => quote! { Self::#id },
                },
                docs: docs::parse_variant(v.attrs, span)?,
            })
        })
        .collect::<Result<_>>()?;

    Ok(CommandSet::Multi(outer, vars))
}
