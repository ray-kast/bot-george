use crate::Result;
use anyhow::anyhow;
use syn::{spanned::Spanned, Attribute, Meta, NestedMeta};

#[derive(Debug)]
pub struct FieldOpts {
    pub subcommand: bool,
}

pub trait ParseOpts: Sized {
    fn parse_opts(attr: &Attribute) -> Result<Self>;

    fn no_opts() -> Result<Self, anyhow::Error>;
}

impl ParseOpts for FieldOpts {
    fn parse_opts(attr: &Attribute) -> Result<Self> {
        let meta = attr.parse_meta().map_err(|e| (e.into(), attr.span()))?;
        let mut ret = Self::default();

        match meta {
            Meta::List(l) => {
                for item in l.nested {
                    match item {
                        NestedMeta::Meta(Meta::Path(p)) if p.is_ident("subcommand") => {
                            if ret.subcommand {
                                return Err((anyhow!("duplicate subcommand specifier"), p.span()));
                            }

                            ret.subcommand = true;
                        },
                        i => {
                            return Err((
                                anyhow!("unexpected value in #[docbot] attribute"),
                                i.span(),
                            ))
                        },
                    }
                }
            },
            _ => {
                return Err((
                    anyhow!("invalid #[docbot] attribute format, expected #[docbot(...)]",),
                    attr.span(),
                ))
            },
        }

        Ok(ret)
    }

    fn no_opts() -> Result<Self, anyhow::Error> { Ok(Default::default()) }
}

impl Default for FieldOpts {
    fn default() -> Self { Self { subcommand: false } }
}

impl ParseOpts for () {
    fn parse_opts(attr: &Attribute) -> Result<Self> {
        Err((anyhow!("unexpected #[docbot] attribute"), attr.span()))
    }

    fn no_opts() -> Result<Self, anyhow::Error> { Ok(()) }
}
