#[allow(clippy::wildcard_imports)]
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

fn parse_ambiguous(span: Span, s: impl ToTokens, values: Vec<&str>) -> impl ToTokens {
    let expected = values.into_iter().map(|v| Literal::string(v));

    quote_spanned! { span => Err(::docbot::IdParseError::Ambiguous(&[#(#expected),*], #s.into())) }
}

fn parse_resolve_ambiguous<'a, 'b, T: Eq + 'b>(values: Vec<&'a (String, T)>) -> Option<&'a T> {
    let mut iter = values.into_iter();

    let first = match iter.next() {
        Some((_, ref i)) => i,
        None => return None,
    };

    if iter.any(|(_, i)| i != first) {
        return None;
    }

    Some(first)
}

fn bits<'a>(
    input: &'a InputData,
) -> Result<(
    Ident,
    Option<TokenStream>,
    Option<&'a Generics>,
    TokenStream,
)> {
    let ty;
    let def;
    let generics;
    let get_fn;

    if match input.commands {
        Commands::Struct(
            _,
            Command {
                fields: Fields::Unit,
                ..
            },
        ) => true,
        Commands::Struct(..) => false,
        Commands::Enum(_, ref vars) => vars
            .iter()
            .all(|v| matches!(v.command.fields, Fields::Unit)),
    } {
        ty = input.ty.clone();
        def = None;
        generics = Some(input.generics);
        get_fn = quote_spanned! { input.span => *self }
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
            Commands::Enum(_, ref vars) => {
                let id_vars = vars.iter().map(|CommandVariant { span, ident, .. }| {
                    let doc = Literal::string(&format!("Identifier for {}::{}", input.ty, ident));

                    quote_spanned! { *span => #[doc = #doc] #ident }
                });

                data = quote_spanned! { input.span => enum #ty { #(#id_vars),* } };

                let id_arms = vars.iter().map(
                    |CommandVariant {
                         span, pat, ident, ..
                     }| {
                        quote_spanned! { *span => #pat => #ty::#ident }
                    },
                );

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

    Ok((ty, def, generics, get_fn))
}

pub fn emit(input: &InputData) -> Result<IdParts> {
    let (ty, def, generics, get_fn) = bits(input)?;

    let parse_s = Ident::new("__str", input.span);
    let parse_iter = Ident::new("__iter", input.span);

    let lexer = match input.commands {
        Commands::Struct(_, Command { ref docs, .. }) => {
            Trie::new(docs.usage.ids.iter().map(|i| (i, ())))
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
        Commands::Enum(_, ref vars) => Trie::new(
            vars.iter()
                .flat_map(|v| v.command.docs.usage.ids.iter().map(move |i| (i, v.ident))),
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

    let display_arms = match input.commands {
        Commands::Struct(_, Command { ref docs, .. }) => {
            let value = Literal::string(&docs.usage.ids[0]);
            vec![quote_spanned! { input.span => Self => #value }]
        },
        Commands::Enum(_, ref vars) => vars
            .iter()
            .map(|CommandVariant { ident, command, .. }| {
                let value = Literal::string(&command.docs.usage.ids[0]);
                quote_spanned! { input.span => Self::#ident => #value }
            })
            .collect(),
    };

    // Quote variables
    let (impl_vars, ty_vars, where_clause) = generics.map_or((None, None, None), |generics| {
        let (imp, ty, whr) = generics.split_for_impl();
        (Some(imp), Some(ty), Some(whr))
    });

    let items = quote_spanned! { input.span =>
        #def

        impl #impl_vars ::std::str::FromStr for #ty #ty_vars #where_clause {
            type Err = ::docbot::IdParseError;

            fn from_str(#parse_s: &str) -> Result<Self, Self::Err> {
                let mut #parse_iter = #parse_s.chars();

                #lexer
            }
        }

        impl #impl_vars ::std::fmt::Display for #ty #ty_vars #where_clause {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(match self {
                    #(#display_arms),*
                })
            }
        }
    };

    Ok(IdParts { ty, items, get_fn })
}
