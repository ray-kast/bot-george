#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![feature(bindings_after_at, proc_macro_diagnostic)]

//! Derive macro for the docbot crate

mod attrs;
mod bits;
mod docs;
mod opts;
mod trie;

use bits::{id::IdParts, parse::ParseParts};
use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;
use syn::{parse_macro_input, spanned::Spanned, DeriveInput};

pub(crate) type Error = (anyhow::Error, Span);
pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Construct a new command or set of commands from doc comments
#[proc_macro_derive(Docbot, attributes(docbot))]
pub fn derive_docbot(input: TokenStream1) -> TokenStream1 {
    let input = parse_macro_input!(input);

    match derive_docbot_impl(input) {
        Ok(s) => {eprintln!("{}", s);s.into()},
        Err((e, s)) => {
            s.unwrap()
                .error(format!("Macro execution failed:\n{:?}", e))
                .emit();
            TokenStream1::new()
        },
    }
}

fn derive_docbot_impl(input: DeriveInput) -> Result<TokenStream> {
    let inputs = bits::inputs::assemble(&input)?;
    let id_parts = bits::id::emit(&inputs)?;
    let parse_parts = bits::parse::emit(&inputs, &id_parts)?;

    // Quote variables
    let IdParts {
        ty: id_ty,
        items: id_items,
        get_fn: id_get_fn,
    } = id_parts;
    let ParseParts {
        fun: parse_fn,
        iter: parse_iter,
    } = parse_parts;
    let span = input.span();
    let name = input.ident;
    let (impl_vars, ty_vars, where_clause) = input.generics.split_for_impl();

    Ok(quote_spanned! { span =>
        #id_items

        impl #impl_vars ::docbot::Command for #name #ty_vars #where_clause {
            type Id = #id_ty;

            fn parse<
                I: IntoIterator<Item = S>,
                S: AsRef<str>,
            >(#parse_iter: I) -> ::std::result::Result<Self, ::docbot::CommandParseError> {
                #parse_fn
            }

            fn id(&self) -> Self::Id { #id_get_fn }
        }
    })
}
