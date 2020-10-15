use crate::Result;
use anyhow::anyhow;
use lazy_static::lazy_static;
use proc_macro2::Span;
use regex::Regex;

#[derive(Clone, Debug)]
pub enum RestArg {
    None,
    Optional(String),
    Required(String),
}

#[derive(Clone, Debug)]
pub struct CommandSyntax {
    pub ids: Vec<String>,
    pub required: Vec<String>,
    pub optional: Vec<String>,
    pub rest: RestArg,
}

#[derive(Debug)]
pub struct CommandDocs {
    pub span: Span,
    pub syntax: CommandSyntax,
    pub help: String,
    pub argument_help: Vec<(String, String)>,
}

pub struct CommandSetDocs {
    pub span: Span,
    pub help: String,
}

pub trait ParseDocs: Sized {
    fn parse_docs(docs: Vec<(String, Span)>) -> Result<Self>;

    fn no_docs() -> Result<Self, anyhow::Error>;
}

lazy_static! {
    static ref COMMAND_IDS_RE: Regex = Regex::new(r"^\s*(?:([^\(]\S*)|\(\s*([^\)]*)\))").unwrap();
    static ref PIPE_RE: Regex = Regex::new(r"\s*\|\s*").unwrap();
    static ref REQUIRED_ARG_RE: Regex = Regex::new(r"^\s*<([^>]{0,2}|[^>]*[^>\.]{3})>").unwrap();
    static ref OPTIONAL_ARG_RE: Regex =
        Regex::new(r"^\s*\[([^\]]{0,2}|[^\]]*[^\]\.]{3})\]").unwrap();
    static ref REST_ARG_RE: Regex = Regex::new(r"^\s*(?:<([^>]+)...>|\[([^\]]+)...\])").unwrap();
    static ref TRAILING_RE: Regex = Regex::new(r"\S").unwrap();
}

fn parse_usage_line((input, span): (String, Span)) -> Result<CommandSyntax> {
    let mut input = input.as_str();

    let ids_match = COMMAND_IDS_RE.captures(input).ok_or_else(|| {
        (
            anyhow!("invalid command ID specifier, expected e.g. 'foo' or '(foo|bar)'"),
            span,
        )
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

    if TRAILING_RE.is_match(input) {
        return Err((anyhow!("trailing string {:?}", input), span));
    }

    Ok(CommandSyntax {
        ids,
        required,
        optional,
        rest,
    })
}

impl ParseDocs for CommandDocs {
    fn parse_docs(docs: Vec<(String, Span)>) -> Result<Self> {
        let span = docs
            .iter()
            .map(|(_, s)| s)
            .fold(None, |prev, curr| match prev {
                None => Some(Some(*curr)),
                Some(p) => Some(p.and_then(|p| p.join(*curr))),
            })
            .unwrap()
            .unwrap();

        let mut docs = docs.into_iter();

        let syntax = parse_usage_line(docs.next().unwrap())?;

        Ok(Self {
            span,
            syntax,
            help: "".into(),       // TODO
            argument_help: vec![], // TODO
        })
    }

    fn no_docs() -> Result<Self, anyhow::Error> { Err(anyhow!("missing doc comment for command")) }
}

impl ParseDocs for CommandSetDocs {
    fn parse_docs(docs: Vec<(String, Span)>) -> Result<Self> {
        let span = docs
            .iter()
            .map(|(_, s)| s)
            .fold(None, |prev, curr| match prev {
                None => Some(Some(*curr)),
                Some(p) => Some(p.and_then(|p| p.join(*curr))),
            })
            .unwrap()
            .unwrap();

        Ok(Self {
            span,
            help: "".into(), // TODO
        })
    }

    fn no_docs() -> Result<Self, anyhow::Error> {
        Err(anyhow!("missing doc comment for command set"))
    }
}

impl ParseDocs for () {
    fn parse_docs(_: Vec<(String, Span)>) -> Result<Self> { Ok(()) }

    fn no_docs() -> Result<Self, anyhow::Error> { Ok(()) }
}
