use crate::{commands::BaseCommand, db::DbPool, error::Result};
use docbot::prelude::*;
use lazy_static::lazy_static;
use log::*;
use regex::{Regex};
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::channel::Message,
};

// TODO: this is here because async closures are unstable
macro_rules! stupid_try {
    ($x:expr) => {
        match $x {
            Ok(o) => o,
            Err(e) => {
                error!("{}", e);
                return;
            },
        }
    };
}

lazy_static! {
    static ref WORD_FINI_RE: Regex = Regex::new(r"\w$").unwrap();
    static ref COMMAND_ARG_RE: Regex =
        Regex::new(r#"\s*(?:([^'"]\S*)|'([^']*)'|"((?:[^"\\]|\\.)*)")"#).unwrap();
    static ref COMMAND_DQUOTE_ESCAPE_RE: Regex = Regex::new(r"\\(.)").unwrap();
}

pub struct Handler {
    prefix_re: Regex,
    superuser: u64,
    pool: DbPool,
}

impl Handler {
    pub fn new(prefix: impl AsRef<str>, superuser: u64, pool: DbPool) -> Result<Self> {
        let prefix_re = Regex::new(&format!(
            r"^\s*{}{}",
            regex::escape(prefix.as_ref()),
            if WORD_FINI_RE.is_match(prefix.as_ref()) {
                r"\b"
            } else {
                ""
            }
        ))?;

        return Ok(Self {
            prefix_re,
            superuser,
            pool,
        });
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            info!("Ignoring message from bot {:?}", msg.author);
            return;
        }

        if let Some(mat) = self.prefix_re.find(&msg.content) {
            let rest = &msg.content[mat.end()..];

            stupid_try!(
                msg.channel_id
                    .send_message(&ctx.http, |m| m.content(format!("```{:?}```", rest)))
                    .await
            );

            let toks = COMMAND_ARG_RE.captures_iter(rest).map(|cap| {
                if let Some(dquot) = cap.get(3) {
                    COMMAND_DQUOTE_ESCAPE_RE.replace_all(dquot.as_str(), "$1")
                } else if let Some(squot) = cap.get(2) {
                    squot.as_str().into()
                } else {
                    cap.get(1).unwrap().as_str().into()
                }
            }).collect::<Vec<_>>();

            let command = BaseCommand::parse(toks);

            match command {
                Ok(c) => stupid_try!(
                    msg.channel_id
                        .send_message(ctx.http, |m| m.content(format!("```{:?}```", c)))
                        .await
                ),
                Err(e) => stupid_try!(
                    msg.channel_id
                        .send_message(ctx.http, |m| m.content(format!("**```{:#?}```**", e)))
                        .await
                ),
            };
        } else {
            // TODO: identify if the message is Important(tm)
        }
    }
}
