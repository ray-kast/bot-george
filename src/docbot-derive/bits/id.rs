use super::inputs::*;
use crate::{trie::Trie, Result};
use proc_macro2::{Literal, TokenStream};
use quote::{quote_spanned, ToTokens};

pub struct IdParts {
    pub ty: Ident,
    pub items: TokenStream,
    pub get_fn: TokenStream,
}

fn parse_no_match(span: Span, s: impl ToTokens) -> impl ToTokens {
    quote_spanned! { span => Err(::docbot::IdParseError::NoMatch(#s.into())) }
}

fn parse_ambiguous(span: Span, s: impl ToTokens, vals: Vec<&str>) -> impl ToTokens {
    let expected = vals.into_iter().map(|v| Literal::string(v));

    quote_spanned! { span => Err(::docbot::IdParseError::Ambiguous(&[#(#expected),*], #s.into())) }
}

fn parse_resolve_ambiguous<'a, 'b, T: Eq + 'b>(vals: Vec<&'a (String, T)>) -> Option<&'a T> {
    let mut iter = vals.into_iter();

    let first = match iter.next() {
        Some((_, ref i)) => i,
        None => return None,
    };

    if let Some(_) = iter.find(|(_, i)| i != first) {
        return None;
    }

    Some(first)
}

pub fn emit(input: &InputData) -> Result<IdParts> {
    let ty;
    let def;
    let generics;
    let get_fn;

    if match input.commands {
        Commands::Struct(Command {
            fields: Fields::Unit,
            ..
        }) => true,
        Commands::Enum(ref vars) => vars.iter().all(|v| {
            if let Fields::Unit = v.command.fields {
                true
            } else {
                false
            }
        }),
        _ => false,
    } {
        ty = input.ty.clone();
        def = None;
        generics = Some(input.generics.split_for_impl());
        get_fn = quote_spanned! { input.span => return *self; }
    } else {
        let vis = input.vis;
        let doc = Literal::string(&format!("Identifier for commands of type {}", input.ty));

        ty = Ident::new(&format!("{}Id", input.ty), input.ty.span());

        let data;

        match input.commands {
            Commands::Struct(..) => {
                data = quote_spanned! { input.span => struct #ty; };
                get_fn = quote_spanned! { input.span => #ty };
            },
            Commands::Enum(ref vars) => {
                let id_vars = vars.iter().map(|CommandVariant { span, ident, .. }| {
                    let doc = Literal::string(&format!("Identifier for {}::{}", input.ty, ident));

                    quote_spanned! { *span => #[doc = #doc] #ident }
                });

                data = quote_spanned! { input.span => enum #ty { #(#id_vars),* } };

                let id_arms = vars.iter().map(|CommandVariant { span, pat, ident, .. }| {
                    quote_spanned! { *span => #pat => #ty::#ident }
                });

                get_fn = quote_spanned! { input.span => match self { #(#id_arms),* } };
            },
        };

        def = Some(quote_spanned! { input.span =>
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            #[doc = #doc]
            #vis #data
        });
        generics = None;
    }

    let parse_s = Ident::new("_s", input.span);
    let parse_iter = Ident::new("_it", input.span);

    let lexer = match input.commands {
        Commands::Struct(Command { ref docs, .. }) => {
            Trie::new(docs.syntax.ids.iter().map(|i| (i, ())))
                .map_err(|e| (e.context("failed to construct command lexer"), input.span))?
                .root()
                .to_lexer(
                    &parse_iter,
                    |()| quote_spanned! { input.span => Ok(#ty) },
                    || parse_no_match(input.span, &parse_s),
                    |v| parse_ambiguous(input.span, &parse_s, v),
                    parse_resolve_ambiguous,
                )
        },
        Commands::Enum(ref vars) => Trie::new(
            vars.iter()
                .flat_map(|v| v.command.docs.syntax.ids.iter().map(move |i| (i, v.ident))),
        )
        .map_err(|e| (e.context("failed to construct command lexer"), input.span))?
        .root()
        .to_lexer(
            &parse_iter,
            |i| quote_spanned! { input.span => Ok(#ty::#i) },
            || parse_no_match(input.span, &parse_s),
            |v| parse_ambiguous(input.span, &parse_s, v),
            parse_resolve_ambiguous,
        ),
    };

    // Quote variables
    let (impl_vars, ty_vars, where_clause) =
        generics.map_or((None, None, None), |g| (Some(g.0), Some(g.1), Some(g.2)));

    let items = quote_spanned! { input.span =>
        #def

        impl #impl_vars ::std::str::FromStr for #ty #ty_vars #where_clause {
            type Err = ::docbot::IdParseError;

            fn from_str(#parse_s: &str) -> Result<Self, Self::Err> {
                let mut #parse_iter = _s.chars();

                #lexer
            }
        }
    };

    Ok(IdParts { ty, items, get_fn })
}
