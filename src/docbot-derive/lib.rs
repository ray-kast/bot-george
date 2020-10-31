#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]
#![feature(bindings_after_at, proc_macro_diagnostic)]

//! Derive macro for the docbot crate

mod attrs;
mod bits;
mod docs;
mod opts;
mod trie;

use bits::{help::HelpParts, id::IdParts, parse::ParseParts};
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
        Ok(s) => s.into(),
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
    let help_parts = bits::help::emit(&inputs)?;

    // Quote variables
    let IdParts {
        items: id_items, ..
    } = id_parts;
    let ParseParts { items: parse_items } = parse_parts;
    let HelpParts { items: help_items } = help_parts;

    Ok(quote_spanned! { input.span() =>
        #id_items
        #parse_items
        #help_items
    })
}
