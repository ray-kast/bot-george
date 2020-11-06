use crate::Result;
use anyhow::anyhow;
use lazy_static::lazy_static;
use proc_macro2::Span;
use regex::{Regex, RegexBuilder};
use std::collections::{hash_map::Entry, HashMap};

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
    pub summary: Option<String>,
    pub args: Vec<(String, bool, String)>,
    pub examples: Option<String>,
}

pub struct CommandSetDocs {
    pub span: Span,
    pub summary: Option<String>,
}

pub trait ParseDocs: Sized {
    fn parse_docs(docs: Vec<(String, Span)>) -> Result<Self>;

    fn no_docs() -> Result<Self, anyhow::Error>;
}

fn take_paragraph<I: Iterator<Item = (String, Span)>>(
    docs: &mut I,
    preserve_lines: bool,
) -> Option<String>
{
    let mut ret = String::new();
    let mut first = true;

    for (string, _) in docs {
        let trimmed = string.trim();

        if trimmed.is_empty() {
            break;
        }

        first = false;

        if preserve_lines {
            ret.push_str(string.as_ref());
            ret.push('\n');
        } else {
            if !ret.is_empty() {
                ret.push(' ');
            }

            ret.push_str(trimmed);
        }
    }

    if first {
        None
    } else {
        Some(ret)
    }
}

fn parse_usage_line((input, span): (String, Span), desc: String) -> Result<CommandUsage> {
    lazy_static! {
        static ref COMMAND_IDS_RE: Regex =
            Regex::new(r"^\s*(?:([^\(]\S*)|\(\s*([^\)]*)\))").unwrap();
        static ref PIPE_RE: Regex = Regex::new(r"\s*\|\s*").unwrap();
        static ref REQUIRED_ARG_RE: Regex =
            Regex::new(r"^\s*<([^>]{0,2}|[^>]*[^>\.]{3})>").unwrap();
        static ref OPTIONAL_ARG_RE: Regex =
            Regex::new(r"^\s*\[([^\]]{0,2}|[^\]]*[^\]\.]{3})\]").unwrap();
        static ref REST_ARG_RE: Regex =
            Regex::new(r"^\s*(?:<([^>]+)...>|\[([^\]]+)...\])").unwrap();
        static ref TRAILING_RE: Regex = Regex::new(r"\S").unwrap();
    }

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
        vec![ids_match[1].into()]
    };

    input = &input[ids_match.get(0).unwrap().end()..];

    let mut required = vec![];
    while let Some(req) = REQUIRED_ARG_RE.captures(input) {
        required.push(req[1].into());

        input = &input[req.get(0).unwrap().end()..];
    }

    let mut optional = vec![];
    while let Some(opt) = OPTIONAL_ARG_RE.captures(input) {
        optional.push(opt[1].into());

        input = &input[opt.get(0).unwrap().end()..];
    }

    let rest = REST_ARG_RE.captures(input).map_or(RestArg::None, |rest| {
        input = &input[rest.get(0).unwrap().end()..];

        rest.get(2).map_or_else(
            || RestArg::Required(rest[1].into()),
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
        desc,
    })
}

fn relax_lines(s: impl AsRef<str>) -> String {
    lazy_static! {
        static ref LINE_RE: Regex = Regex::new(r"\s*\n\s*").unwrap();
    }

    LINE_RE.replace_all(s.as_ref().trim(), " ").into_owned()
}

fn parse_argument_lines(
    span: Span,
    usage: &CommandUsage,
    s: impl AsRef<str>,
) -> Result<Vec<(String, bool, String)>>
{
    lazy_static! {
        static ref ARGUMENT_RE: Regex =
            RegexBuilder::new(r"^\s*([^\n\s](?:[^:\n]*[^:\n\s])?)\s*:\s*")
                .multi_line(true)
                .build()
                .unwrap();
    }

    let s = s.as_ref();
    let mut args = HashMap::new();
    let matches: Vec<_> = ARGUMENT_RE.captures_iter(s).collect();

    for (i, caps) in matches.iter().enumerate() {
        let end = matches
            .get(i + 1)
            .map_or_else(|| s.len(), |c| c.get(0).unwrap().start());

        match args.entry(caps[1].into()) {
            Entry::Occupied(o) => {
                return Err((
                    anyhow!("duplicate argument description {:?}", o.key()),
                    span,
                ))
            },
            Entry::Vacant(v) => {
                v.insert(relax_lines(&s[caps.get(0).unwrap().end()..end]));
            },
        }
    }

    if !s.is_empty() && args.is_empty() {
        return Err((anyhow!("unexpected argument description format"), span));
    }

    let expected_args: Vec<_> = usage
        .required
        .iter()
        .map(|a| (a, true))
        .chain(usage.optional.iter().map(|a| (a, false)))
        .chain(match usage.rest {
            RestArg::None => None,
            RestArg::Optional(ref a) => Some((a, false)),
            RestArg::Required(ref a) => Some((a, true)),
        })
        .collect();

    for (arg, _) in &expected_args {
        if args.get(*arg).is_none() {
            return Err((
                anyhow!(
                    "missing documentation for argument {:?} (have documentation for {:?})",
                    arg,
                    args.keys().collect::<Vec<_>>(),
                ),
                span,
            ));
        }
    }

    if args.len() != expected_args.len() {
        return Err((
            anyhow!(
                "mismatched number of argument descriptions (expected {}, got {})",
                expected_args.len(),
                args.len()
            ),
            span,
        ));
    }

    let args = expected_args
        .into_iter()
        .map(|(arg, req)| {
            let (key, val) = args.remove_entry(arg).unwrap();
            (key, req, val)
        })
        .collect();

    Ok(args)
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

        let usage = docs.next().unwrap();
        let desc = take_paragraph(&mut docs, false).unwrap_or_else(String::new);

        let usage = parse_usage_line(usage, desc)?;

        let mut summary = None;
        let mut args = None;
        let mut examples = None;

        while let Some(par) = take_paragraph(&mut docs, true) {
            lazy_static! {
                static ref HEADER_RE: Regex = Regex::new(r"^\s*#\s*(\S+)\s*\n").unwrap();
            }

            let header_caps = HEADER_RE
                .captures(&par)
                .ok_or_else(|| (anyhow!("paragraph missing header"), span))?;
            let rest = &par[header_caps.get(0).unwrap().end()..];

            match header_caps[1].to_lowercase().as_ref() {
                "description" | "overview" | "summary" => {
                    if summary.is_some() {
                        return Err((anyhow!("multiple summary sections found"), span));
                    }

                    summary = Some(relax_lines(rest))
                },
                "arguments" | "parameters" => {
                    if args.is_some() {
                        return Err((anyhow!("multiple arguments sections found"), span));
                    }

                    args = Some(parse_argument_lines(span, &usage, rest)?)
                },
                "examples" => {
                    if examples.is_some() {
                        return Err((anyhow!("multiple examples sections found"), span));
                    }

                    examples = Some(relax_lines(rest))
                },
                _ => (),
            }
        }

        if usage.desc.trim().is_empty() {
            return Err((anyhow!("missing command description"), span));
        }

        let args = args.map_or_else(|| parse_argument_lines(span, &usage, ""), Ok)?;

        Ok(Self {
            span,
            usage,
            summary,
            args,
            examples,
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

        let mut docs = docs.into_iter();
        let mut summary = String::new();

        while let Some(par) = take_paragraph(&mut docs, false) {
            if !summary.is_empty() {
                summary.push('\n');
            }

            summary.push_str(&par);
        }

        let summary = summary.trim();
        let summary = if summary.is_empty() {
            Some(summary.into())
        } else {
            None
        };

        Ok(Self { span, summary })
    }

    fn no_docs() -> Result<Self, anyhow::Error> {
        Ok(Self {
            span: Span::call_site(),
            summary: None,
        })
    }
}

impl ParseDocs for () {
    fn parse_docs(_: Vec<(String, Span)>) -> Result<Self> { Ok(()) }

    fn no_docs() -> Result<Self, anyhow::Error> { Ok(()) }
}
