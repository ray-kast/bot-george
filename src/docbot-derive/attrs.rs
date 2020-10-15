use crate::{
    docs::{CommandDocs, ParseDocs},
    opts::{FieldOpts, ParseOpts},
    Result,
};
use anyhow::{anyhow, Context};
use proc_macro2::Span;
use syn::{spanned::Spanned, Attribute, Lit, Meta, MetaNameValue};

fn parse_core<O: ParseOpts, D: ParseDocs>(attrs: &Vec<Attribute>, span: Span) -> Result<(O, D)> {
    let mut opts = None;
    let mut docs = Vec::new();

    for attr in attrs {
        match attr.path.get_ident() {
            Some(i) if i == "doc" => {
                let meta = attr
                    .parse_meta()
                    .context("failed to parse doc comment")
                    .map_err(|e| (e, attr.span()))?;

                if let Meta::NameValue(MetaNameValue {
                    lit: ref l @ Lit::Str(ref s),
                    ..
                }) = meta
                {
                    docs.push((s.value(), l.span()));
                } else {
                    return Err((anyhow!("unexpected doc comment format"), attr.span()));
                }
            },
            Some(i) if i == "docbot" => {
                if let Some(..) = &opts {
                    return Err((anyhow!("multiple #[docbot] attributes found"), attr.span()));
                }

                opts = Some(O::parse_opts(attr)?);
            },
            _ => (),
        }
    }

    return Ok((
        opts.map_or_else(|| O::no_opts(), |o| Ok(o))
            .map_err(|e| (e, span))?,
        if docs.len() == 0 {
            D::no_docs().map_err(|e| (e, span))
        } else {
            D::parse_docs(docs)
        }?,
    ));
}

pub fn parse_outer<D: ParseDocs>(attrs: &Vec<Attribute>, span: Span) -> Result<D> {
    let ((), ret) = parse_core(attrs, span)?;
    return Ok(ret);
}

pub fn parse_variant(attrs: &Vec<Attribute>, span: Span) -> Result<CommandDocs> {
    let ((), ret) = parse_core(attrs, span)?;
    return Ok(ret);
}

pub fn parse_field(attrs: &Vec<Attribute>, span: Span) -> Result<FieldOpts> {
    let (ret, ()) = parse_core(attrs, span)?;
    return Ok(ret);
}
