use crate::Result;
use anyhow::{anyhow, Context};
use lazy_static::lazy_static;
use proc_macro2::Span;
use regex::Regex;
use syn::{spanned::Spanned, Attribute, Lit, Meta, MetaNameValue};

#[derive(Debug)]
pub enum RestArg {
    None,
    Optional(String),
    Required(String),
}

#[derive(Debug)]
pub struct CommandDocs {
    pub help: String,
    pub ids: Vec<String>,
    pub required: Vec<String>,
    pub optional: Vec<String>,
    pub rest: RestArg,
    pub argument_help: Vec<(String, String)>,
}

pub struct CommandSetDocs {
    pub help: String,
}

pub trait ParseDocs: Sized {
    fn parse(doc: String) -> Result<Self, anyhow::Error>;
}

lazy_static! {
    static ref COMMAND_IDS_RE: Regex = Regex::new(r"^\s*(?:([^\(]\S*)|\(\s*([^\)]*)\))").unwrap();
    static ref PIPE_RE: Regex = Regex::new(r"\s*\|\s*").unwrap();
    static ref REQUIRED_ARG_RE: Regex = Regex::new(r"^\s*<([^>]{0,2}|[^>]*[^>\.]{3})>").unwrap();
    static ref OPTIONAL_ARG_RE: Regex =
        Regex::new(r"^\s*\[([^\]]{0,2}|[^\]]*[^\]\.]{3})\]").unwrap();
    static ref REST_ARG_RE: Regex = Regex::new(r"^\s*(?:<([^>]+)...>|\[([^\]]+)...\])").unwrap();
}

impl ParseDocs for CommandDocs {
    fn parse(doc: String) -> Result<Self, anyhow::Error> {
        let mut input = doc.as_str();

        let ids_match = COMMAND_IDS_RE.captures(input).ok_or_else(|| {
            anyhow!("invalid command ID specifier, expected e.g. 'foo' or '(foo|bar)'")
        })?;

        let ids = if let Some(cap) = ids_match.get(2) {
            PIPE_RE.split(cap.as_str()).map(|s| s.into()).collect()
        } else {
            vec![ids_match.get(1).unwrap().as_str().into()]
        };

        input = &input[ids_match.get(0).unwrap().end()..];

        let mut required = vec![];
        while let Some(req) = REQUIRED_ARG_RE.captures(input) {
            required.push(req.get(1).unwrap().as_str().into());

            input = &input[req.get(0).unwrap().end()..];
        }

        let mut optional = vec![];
        while let Some(opt) = OPTIONAL_ARG_RE.captures(input) {
            optional.push(opt.get(1).unwrap().as_str().into());

            input = &input[opt.get(0).unwrap().end()..];
        }

        let rest = if let Some(rest) = REST_ARG_RE.captures(input) {
            input = &input[rest.get(0).unwrap().end()..];

            if let Some(cap) = rest.get(2) {
                RestArg::Optional(cap.as_str().into())
            } else {
                RestArg::Required(rest.get(1).unwrap().as_str().into())
            }
        } else {
            RestArg::None
        };

        Ok({ let x = Self {
            ids,
            required,
            optional,
            rest,
            argument_help: vec![], // TODO
            help: input.into(),    // TODO
        }; eprintln!("{:#?}", x); x }) // TODO
    }
}

impl ParseDocs for CommandSetDocs {
    fn parse(doc: String) -> Result<Self, anyhow::Error> {
        // TODO
        Ok(Self { help: doc })
    }
}

fn parse_core<F: FnOnce(String) -> Result<T, anyhow::Error>, T>(
    attrs: Vec<Attribute>,
    span: Span,
    desc: &'static str,
    fun: F,
) -> Result<T>
{
    for attr in attrs {
        match attr.path.get_ident() {
            Some(i) if i == "doc" => {
                let meta = attr
                    .parse_meta()
                    .context("failed to parse doc comment")
                    .map_err(|e| (e, attr.span()))?;

                return if let Meta::NameValue(MetaNameValue {
                    lit: ref l @ Lit::Str(ref s),
                    ..
                }) = meta
                {
                    return fun(s.value()).map_err(|e| (e, l.span()));
                } else {
                    Err((anyhow!("unexpected doc comment format"), attr.span()))
                };
            },
            Some(i) if i == "docbot" => attr
                .span()
                .unwrap()
                .error("unexpected #[docbot] attribute")
                .emit(),
            _ => (),
        }
    }

    Err((anyhow!("missing {} doc comment", desc), span))
}

pub fn parse_outer<P: ParseDocs>(attrs: Vec<Attribute>, span: Span) -> Result<P> {
    return parse_core(attrs, span, "outer", P::parse);
}

pub fn parse_variant(attrs: Vec<Attribute>, span: Span) -> Result<CommandDocs> {
    return parse_core(attrs, span, "variant", CommandDocs::parse);
}
