use crate::{
    bot::{channels, roles},
    commands,
    commands::BaseCommand,
    db::DbPool,
    error::Result,
    util::MessageBuilderExt,
};
use anyhow::Context as _;
use dispose::defer;
use docbot::{prelude::*, ArgumentUsage, CommandUsage, HelpTopic};
use lazy_static::lazy_static;
use log::{error, info};
use regex::Regex;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    http::Http,
    model::{
        channel::{Channel, Message},
        gateway::{Activity, Ready},
        id::{ChannelId, UserId},
        user::OnlineStatus,
    },
    utils::MessageBuilder,
};
use std::{
    fmt::{Display, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};
use tokio::runtime;

// TODO: this is here because async closures are unstable
macro_rules! stupid_try {
    ($x: expr) => {
        match $x {
            Ok(o) => o,
            Err(..) => return,
        }
    };
    ($x:expr, $e:ident => $err: expr) => {
        match $x {
            Ok(o) => o,
            Err($e) => return $err,
        }
    };
    ($x:expr, _ => $err: expr) => {
        match $x {
            Ok(o) => o,
            Err(..) => return $err,
        }
    };
}

lazy_static! {
    static ref WORD_END_RE: Regex = Regex::new(r"\w$").unwrap();
}

pub struct Handler {
    prefix: String,
    prefix_re: Regex,
    superuser: UserId,
    pool: DbPool,
    me: AtomicU64,
}

impl Handler {
    pub fn new(prefix: impl AsRef<str>, superuser: UserId, pool: DbPool) -> Result<Self> {
        let prefix_re = Regex::new(&format!(
            r"^\s*{}{}",
            regex::escape(prefix.as_ref()),
            if WORD_END_RE.is_match(prefix.as_ref()) {
                r"\b"
            } else {
                ""
            }
        ))?;

        return Ok(Self {
            prefix: prefix.as_ref().into(),
            prefix_re,
            superuser,
            pool,
            me: 0.into(),
        });
    }

    pub fn prefix_command<C: Display>(&self, command: C) -> String {
        let mut ret = String::new();

        write!(ret, "{}{}", self.prefix, command).unwrap();

        if !self.prefix_re.is_match(&ret) {
            ret.clear();

            write!(ret, "{} {}", self.prefix, command).unwrap();

            // If neither of these work then we're in trouble
            assert!(self.prefix_re.is_match(&ret));
        }

        ret
    }

    async fn send_err_message(chan: ChannelId, http: impl AsRef<Http>, err: anyhow::Error) {
        error!("{:?}", err);
        chan.say(
            http,
            MessageBuilder::new()
                .push("**ERROR:**")
                .push_codeblock_safe(format!("{:?}", err), None),
        )
        .await
        .map_err(|e| error!("error while reporting error: {:?}", e))
        .ok();
    }

    async fn send_guild_required(channel_id: ChannelId, http: impl AsRef<Http>) -> Result<()> {
        channel_id
            .say(
                http,
                "**ERROR:** This command cannot be used in a DM channel.",
            )
            .await
            .context("failed to send guild ID error message")?;

        Ok(())
    }

    async fn send_no_permission(
        channel_id: ChannelId,
        http: impl AsRef<Http>,
        err: roles::NoPermissionError,
    ) -> Result<()>
    {
        use roles::NoPermissionError::{Add, Remove, Show};

        channel_id
            .say(
                http,
                format!(
                    "**ERROR:** You do not have permission to {}",
                    match err {
                        Show => "show assigned roles".into(),
                        Add(r) => format!("add the role **{}**", r),
                        Remove(r) => format!("remove the role **{}**", r),
                    }
                ),
            )
            .await
            .context("failed to send error message")?;

        Ok(())
    }

    async fn send_help(
        channel_id: ChannelId,
        http: impl AsRef<Http>,
        help: &HelpTopic,
    ) -> Result<()>
    {
        fn format_arg_usage(usage: &ArgumentUsage) -> String {
            let mut ret = String::new();

            ret.push(if usage.is_required { '<' } else { '[' });
            ret.write_str(usage.name).unwrap();
            if usage.is_rest {
                ret.write_str("...").unwrap();
            }
            ret.push(if usage.is_required { '>' } else { ']' });

            ret
        }

        fn format_usage(usage: &CommandUsage) -> String {
            let mut ret = Vec::new();

            {
                let mut ids = String::new();

                lazy_static! {
                    static ref NONWORD_RE: Regex = Regex::new(r"\s").unwrap();
                }

                let paren = usage.ids.len() != 1 || NONWORD_RE.is_match(usage.ids.first().unwrap());

                if paren {
                    ids.push('(');
                }

                write!(ids, "{}", usage.ids.join("|")).unwrap();

                if paren {
                    ids.push(')');
                }

                ret.push(ids);
            }

            ret.extend(usage.args.iter().map(|a| format_arg_usage(a)));

            ret.join(" ")
        }

        channel_id
            .send_message(http, |msg| match help {
                HelpTopic::Command(u, d) => msg.content(
                    MessageBuilder::new()
                        .push("**Usage:** ")
                        .push_line(format_usage(u))
                        .push_mono_safer(format!("{:?}", d)),
                ),
                HelpTopic::CommandSet(s, c) => {
                    msg.content(s);

                    // TODO
                    msg
                },
                HelpTopic::Custom(s) => msg.content(s),
            })
            .await
            .context("failed to send help")?;

        Ok(())
    }

    fn format_id_error(err: docbot::IdParseError) -> String {
        use docbot::IdParseError::{Ambiguous, NoMatch};

        let mut b = MessageBuilder::new();

        match err {
            // TODO: did-you-mean
            NoMatch(s) => b.push("Not sure what you mean by ").push_mono_safer(s),
            Ambiguous(v, i) => {
                b.push("Not sure what you mean by ")
                    .push_mono_safer(i)
                    .push(", could be ");

                for (i, v) in v.iter().enumerate() {
                    if i != 0 {
                        b.push(", ");
                    }

                    b.push_mono_safer(v);
                }

                &mut b
            },
        }
        .build()
    }

    fn format_cmd_error(err: docbot::CommandParseError) -> String {
        use docbot::CommandParseError::{
            BadConvert, BadId, MissingRequired, NoInput, Subcommand, Trailing,
        };

        let mut b = MessageBuilder::new();

        // TODO: recommend running a help command

        match err {
            NoInput => b.push("Expected a command, got nothing"),
            BadId(e) => b.push(Self::format_id_error(e)),
            MissingRequired(a) => b.push("Missing required argument ").push_mono_safer(a),
            BadConvert(a, e) => {
                enum Downcast {
                    Cmd(docbot::CommandParseError),
                    Id(docbot::IdParseError),
                    Other(anyhow::Error),
                }

                b.push("Failed to process argument ")
                    .push_mono_safer(a)
                    .push(": ");

                match e.downcast().map_or_else(
                    |e| e.downcast().map_or_else(Downcast::Other, Downcast::Id),
                    Downcast::Cmd,
                ) {
                    Downcast::Cmd(e) => b.push(Self::format_cmd_error(e)),
                    Downcast::Id(e) => b.push(Self::format_id_error(e)),
                    Downcast::Other(e) => b.push_safe(e),
                }
            },
            Trailing(s) => b
                .push("Too many arguments given (starting with ")
                .push_mono_safer(s)
                .push(")"),
            Subcommand(e) => b
                .push("Subcommand failed: ")
                .push(Self::format_cmd_error(*e)),
        }
        .build()
    }

    async fn handle_command<S: AsRef<str>>(&self, s: S, ctx: Context, msg: &Message) -> Result<()> {
        use BaseCommand::{Channel, Help, Modmail, Role, Schedule};

        let chan = msg.channel_id;
        let http = Arc::clone(&ctx.http);

        let _d = defer(|| {
            if thread::panicking() {
                tokio::task::spawn_blocking(move || {
                    runtime::Builder::new()
                        .enable_all()
                        .basic_scheduler()
                        .build()
                        .unwrap()
                        .block_on(chan.say(
                            http,
                            "**ERROR:** thread panicked while servicing your request",
                        ))
                        .ok()
                });
            }
        });

        let cmd = match commands::parse_base(s) {
            Ok(c) => c,
            Err(e) => {
                chan.say(ctx, format!("**ERROR:** {}", Self::format_cmd_error(e)))
                    .await
                    .context("failed to send command parse error")?;
                return Ok(());
            },
        };

        match cmd {
            Help(topic) => Self::send_help(chan, ctx, BaseCommand::help(topic)).await?,
            Role(role_cmd) => {
                use roles::{
                    RoleCommandError::{GuildRequired, NoPermission, Other},
                    RoleCommandOk::{Added, Help, List, Removed, ShowAll, ShowOne},
                };

                match roles::execute(
                    role_cmd,
                    msg.author.id,
                    msg.guild_id,
                    &self.pool,
                    self.superuser,
                ) {
                    Ok(Help(topic)) => Self::send_help(chan, ctx, topic).await?,
                    Ok(List(())) => todo!(),
                    Ok(ShowAll(_map)) => todo!(),
                    Ok(ShowOne(_user, _roles)) => todo!(),
                    Ok(Added(n)) => {
                        msg.channel_id
                            .say(
                                &ctx,
                                format!("Added {} role{}.", n, if n == 1 { "" } else { "s" }),
                            )
                            .await
                            .context("failed to send success message")?;
                    },
                    Ok(Removed(n)) => {
                        msg.channel_id
                            .say(
                                &ctx,
                                format!("Removed {} role{}.", n, if n == 1 { "" } else { "s" }),
                            )
                            .await
                            .context("failed to send success message")?;
                    },
                    Err(GuildRequired) => Self::send_guild_required(msg.channel_id, &ctx).await?,
                    Err(NoPermission(n)) => {
                        Self::send_no_permission(msg.channel_id, &ctx, n).await?
                    },
                    Err(Other(e)) => Err(e).context("an unexpected error occurred")?,
                }
            },
            Channel(chan_cmd) => {
                use channels::{
                    ChannelCommandError::{GuildRequired, NoPermission, Other},
                    ChannelCommandOk::{Help, List, Marked, ShowAll, ShowOne, Unmarked},
                };

                match channels::execute(
                    chan_cmd,
                    msg.author.id,
                    msg.guild_id,
                    &self.pool,
                    self.superuser,
                ) {
                    Ok(Help(topic)) => Self::send_help(chan, ctx, topic).await?,
                    Ok(List(())) => todo!(),
                    Ok(ShowAll { .. }) => todo!(),
                    Ok(ShowOne { .. }) => todo!(),
                    Ok(Marked) => todo!(),
                    Ok(Unmarked) => todo!(),
                    Err(GuildRequired) => Self::send_guild_required(msg.channel_id, &ctx).await?,
                    Err(NoPermission(n)) => {
                        Self::send_no_permission(msg.channel_id, &ctx, n).await?
                    },
                    Err(Other(e)) => Err(e).context("an unexpected error occurred")?,
                }
            },
            Schedule(_) => todo!(),
            Modmail(_message) => todo!(),
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        self.me.store(*ready.user.id.as_u64(), Ordering::Release);

        ctx.set_presence(
            Some(Activity::playing(&format!(
                "CS:GO | {}",
                self.prefix_command("help")
            ))),
            OnlineStatus::Online,
        )
        .await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let me = self.me.load(Ordering::Acquire);

        if *msg.author.id.as_u64() == me {
            return;
        }

        if msg.author.bot {
            info!("Ignoring message from bot {:?}", msg.author);
            return;
        }

        let spare_http = Arc::clone(&ctx.http);
        let result = if let Some(mat) = self.prefix_re.find(&msg.content) {
            let rest = &msg.content[mat.end()..];

            self.handle_command(rest, ctx, &msg).await
        } else if let Channel::Private(..) = stupid_try!(
            msg.channel_id.to_channel(&ctx).await,
            e => error!("error while getting message channel: {:?}", e)
        ) {
            self.handle_command(&msg.content, ctx, &msg).await
        } else {
            // TODO: identify if the message is Important(tm)
            Ok(())
        };

        stupid_try!(result, e => Self::send_err_message(msg.channel_id, spare_http, e).await);
    }
}
