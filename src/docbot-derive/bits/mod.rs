pub mod id;
pub mod parse;

pub mod inputs {
    use crate::{attrs, Result};
    use anyhow::anyhow;
    use proc_macro2::TokenStream;
    use quote::quote_spanned;
    use syn::{spanned::Spanned, Data, DeriveInput};

    pub use crate::docs::CommandDocs;
    pub use proc_macro2::Span;
    pub use syn::{Fields, Generics, Ident, Variant, Visibility};

    pub struct InputData<'a> {
        pub span: Span,
        pub vis: &'a Visibility,
        pub ty: &'a Ident,
        pub generics: &'a Generics,

        pub outer_docs: (),
        pub commands: Commands<'a>,
    }

    pub enum Commands<'a> {
        Struct(Command<'a>),
        Enum(Vec<CommandVariant<'a>>),
    }

    pub struct Command<'a> {
        pub docs: CommandDocs,
        pub fields: &'a Fields,
    }

    pub struct CommandVariant<'a> {
        pub span: Span,
        pub ident: &'a Ident,
        pub pat: TokenStream,
        pub command: Command<'a>,
    }

    pub fn assemble(input: &DeriveInput) -> Result<InputData> {
        let commands = match input.data {
            Data::Struct(ref s) => Commands::Struct(Command {
                docs: attrs::parse_outer(&input.attrs, input.span())?,
                fields: &s.fields,
            }),
            Data::Enum(ref e) => Commands::Enum(
                e.variants
                    .iter()
                    .map(|v| {
                        Ok(CommandVariant {
                            ident: &v.ident,
                            pat: {
                                let id = &v.ident;
                                match v.fields {
                                    Fields::Named(..) => {
                                        quote_spanned! { v.span() => Self::#id { .. } }
                                    },
                                    Fields::Unnamed(..) => {
                                        quote_spanned! { v.span() => Self::#id(..) }
                                    },
                                    Fields::Unit => quote_spanned! { v.span() => Self::#id },
                                }
                            },
                            command: Command {
                                docs: attrs::parse_variant(&v.attrs, v.span())?,
                                fields: &v.fields,
                            },
                            span: v.span(),
                        })
                    })
                    .collect::<Result<_>>()?,
            ),
            Data::Union(_) => {
                return Err((anyhow!("cannot derive Docbot on a union."), input.span()))
            },
        };

        Ok(InputData {
            span: input.span(),
            vis: &input.vis,
            ty: &input.ident,
            generics: &input.generics,

            outer_docs: (),
            commands,
        })
    }
}
