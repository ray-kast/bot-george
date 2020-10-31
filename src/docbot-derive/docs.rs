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
pub struct CommandUsage {
    pub ids: Vec<String>,
    pub required: Vec<String>,
    pub optional: Vec<String>,
    pub rest: RestArg,
    pub desc: String,
}

#[derive(Debug)]
pub struct CommandDocs {
    pub span: Span,
    pub usage: CommandUsage,
    pub summary: String,
    pub args: Vec<(String, String)>,
    pub examples: Option<String>,
}

pub struct CommandSetDocs {
    pub span: Span,
    pub summary: String,
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

fn parse_usage_line((input, span): (String, Span)) -> Result<CommandUsage> {
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

    let rest = REST_ARG_RE.captures(input).map_or(RestArg::None, |rest| {
        input = &input[rest.get(0).unwrap().end()..];

        rest.get(2).map_or_else(
            || RestArg::Required(rest.get(1).unwrap().as_str().into()),
            |cap| RestArg::Optional(cap.as_str().into()),
        )
    });

    if TRAILING_RE.is_match(input) {
        return Err((anyhow!("trailing string {:?}", input), span));
    }

    Ok(CommandUsage {
        ids,
        required,
        optional,
        rest,
        desc: "".into(), // TODO
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

        let usage = parse_usage_line(docs.next().unwrap())?;

        Ok(Self {
            span,
            usage,
            summary: "".into(), // TODO
            args: vec![],       // TODO
            examples: None,     // TODO
        })
    }

    fn no_docs() -> Result<Self, anyhow::Error> { Err(anyhow!("missing doc comment for command")) }
}

impl ParseDocs for CommandSetDocs {
    fn parse_docs(docs: Vec<(String, Span)>) -> Result<Self> {
        let (summary, span) = docs.iter().fold(
            (String::new(), None),
            |(summary, span), (curr_summary, curr_span)| {
                (
                    format!("{}\n{}", summary, curr_summary),
                    match span {
                        None => Some(Some(*curr_span)),
                        Some(p) => Some(p.and_then(|p| p.join(*curr_span))),
                    },
                )
            },
        );
        let summary = summary.trim().into();
        let span = span.flatten().unwrap();

        Ok(Self { span, summary })
    }

    fn no_docs() -> Result<Self, anyhow::Error> {
        Err(anyhow!("missing doc comment for command set"))
    }
}

impl ParseDocs for () {
    fn parse_docs(_: Vec<(String, Span)>) -> Result<Self> { Ok(()) }

    fn no_docs() -> Result<Self, anyhow::Error> { Ok(()) }
}
