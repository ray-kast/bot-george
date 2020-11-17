use crate::{
    bot::{channels, channels::ChannelCommand, roles, roles::RoleCommand},
    commands,
    commands::BaseCommand,
    db::DbPool,
    error::Result,
    util::MessageBuilderExt,
};
use anyhow::Context as _;
use dispose::defer;
use docbot::{prelude::*, ArgumentDesc, ArgumentName, ArgumentUsage, CommandUsage, HelpTopic};
use lazy_static::lazy_static;
use log::{error, info};
use regex::{Captures, Regex};
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
    borrow::Cow,
    collections::BinaryHeap,
    fmt::{Display, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};
use strsim::normalized_damerau_levenshtein;
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

    // TODO: don't prefix the command if the channel is command-only
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

    fn format_help_content<'a>(&self, s: &'a str) -> Cow<'a, str> {
        lazy_static! {
            static ref COMMAND_RE: Regex = Regex::new(r"\[`([^`]+)`\]\(\)").unwrap();
        }

        COMMAND_RE.replace_all(s, |c: &Captures| {
            let cmd = &c[1];
            assert!(commands::parse_base(cmd).is_ok());

            format!("`{}`", self.prefix_command(cmd))
        })
    }

    fn format_arg_usage(usage: &ArgumentUsage) -> String {
        let mut ret = String::new();

        ret.push(if usage.is_required { '<' } else { '[' });
        ret.push_str(usage.name);
        if usage.is_rest {
            ret.push_str("...");
        }
        ret.push(if usage.is_required { '>' } else { ']' });

        ret
    }

    fn format_command_usage(&self, usage: &CommandUsage, rich: bool) -> String {
        let mut ret = Vec::new();

        {
            let mut ids = String::new();

            lazy_static! {
                static ref NON_WORD_RE: Regex = Regex::new(r"\s").unwrap();
            }

            let paren = usage.ids.len() != 1 || NON_WORD_RE.is_match(usage.ids.first().unwrap());

            if paren {
                ids.push('(');
            }

            write!(ids, "{}", usage.ids.join("|")).unwrap();

            if paren {
                ids.push(')');
            }

            ret.push(ids);
        }

        ret.extend(usage.args.iter().map(|a| Self::format_arg_usage(a)));

        let desc = self.format_help_content(usage.desc);

        if rich {
            format!("**{}**\n{}", ret.join(" "), desc)
        } else {
            format!("{}\n{}", ret.join(" "), desc)
        }
    }

    // TODO: send the reply to a DM if the channel is not a command-only channel
    async fn send_help(
        &self,
        channel_id: ChannelId,
        http: impl AsRef<Http>,
        help: &HelpTopic,
        list_title: impl Display,
    ) -> Result<()>
    {
        channel_id
            .send_message(http, |msg| match help {
                HelpTopic::Command(u, d) => msg
                    .content(format!(
                        "**Usage:** {}",
                        self.format_command_usage(u, false)
                    ))
                    .embed(|e| {
                        e.title("Description").description({
                            enum Block {
                                Par(&'static str),
                                Head(&'static str),
                                Arg(&'static ArgumentDesc),
                            }

                            let mut m = MessageBuilder::new();

                            for (i, block) in d
                                .summary
                                .iter()
                                .map(|s| Block::Par(s))
                                .chain(d.args.first().map(|_| Block::Head("**Arguments**")))
                                .chain(d.args.iter().map(|a| Block::Arg(a)))
                                .chain(d.examples.iter().map(|_| Block::Head("**Examples**")))
                                .chain(d.examples.iter().map(|s| Block::Par(s)))
                                .enumerate()
                            {
                                if i != 0 {
                                    m.push('\n');
                                }

                                match block {
                                    Block::Par(s) => {
                                        m.push_line(self.format_help_content(s));
                                    },
                                    Block::Head(s) => {
                                        m.push(s);
                                    },
                                    Block::Arg(a) => {
                                        m.push(" - ").push_bold_safe(a.name);

                                        if !a.is_required {
                                            m.push(" (optional)");
                                        }

                                        m.push(": ").push_line(self.format_help_content(a.desc));
                                    },
                                }
                            }

                            m
                        })
                    }),
                #[allow(clippy::option_if_let_else)] // Lifetime issues forbid this
                HelpTopic::CommandSet(s, c) => if let Some(s) = s {
                    msg.content(self.format_help_content(s))
                } else {
                    msg
                }
                .embed(|e| {
                    e.title(list_title).description({
                        let mut m = MessageBuilder::new();

                        for (i, cmd) in c.iter().enumerate() {
                            if i != 0 {
                                m.push('\n');
                            }

                            m.push(" - ")
                                .push_line(self.format_command_usage(cmd, true));
                        }

                        m
                    })
                }),
                HelpTopic::Custom(s) => msg.content(self.format_help_content(s)),
            })
            .await
            .context("failed to send help")?;

        Ok(())
    }

    async fn send_version(chan: ChannelId, http: impl AsRef<Http>) -> Result<()> {
        chan.send_message(http, |m| {
            m.content(
                MessageBuilder::new()
                    .push("This is ")
                    .push_safe(env!("CARGO_BIN_NAME"))
                    .push(" v")
                    .push_safe(env!("CARGO_PKG_VERSION"))
                    .push_safe(
                        option_env!("GIT_HEAD").map_or_else(String::new, |h| format!(".git+{}", h)),
                    )
                    .push_safe(
                        option_env!("GIT_REMOTE").map_or_else(String::new, |r| format!(" ({})", r)),
                    ),
            )
            .embed(|e| {
                e.title("Build Configuration").description(
                    MessageBuilder::new()
                        .push_bold("Compiler: ")
                        .push_line_safe(env!("RUSTC_VERSION"))
                        .push_bold("Host: ")
                        .push_line_safe(env!("BUILD_HOST"))
                        .push_bold("Target: ")
                        .push_line_safe(env!("BUILD_TARGET"))
                        .push_bold("Profile: ")
                        .push_line_safe(env!("BUILD_PROFILE"))
                        .push_bold("Features: ")
                        .push_line_safe(env!("BUILD_FEATURES")),
                )
            })
        })
        .await?;

        Ok(())
    }

    fn format_id_error(err: docbot::IdParseError) -> (String, bool) {
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct DidYouMean<S: AsRef<str>>(f64, S);

        use std::cmp::Ordering;

        impl<S: Eq + AsRef<str>> Eq for DidYouMean<S> {}
        impl<S: PartialOrd + AsRef<str>> PartialOrd for DidYouMean<S> {
            fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
                self.0
                    .partial_cmp(&rhs.0)
                    .map(|o| o.then_with(|| rhs.1.partial_cmp(&self.1).unwrap_or(Ordering::Equal)))
            }
        }
        impl<S: Ord + AsRef<str>> Ord for DidYouMean<S> {
            fn cmp(&self, rhs: &Self) -> Ordering { self.partial_cmp(rhs).unwrap() }
        }

        use docbot::IdParseError::{Ambiguous, NoMatch};

        let mut b = MessageBuilder::new();
        let mut has_help = false;

        match err {
            NoMatch(s, v) => {
                b.push("Not sure what you mean by ").push_mono_safer(&s);

                for (i, val) in v
                    .iter()
                    .map(|v| {
                        DidYouMean(
                            normalized_damerau_levenshtein(&s, &v[0..v.len().min(s.len() + 1)]),
                            v,
                        )
                    })
                    .collect::<BinaryHeap<_>>()
                    .into_iter_sorted()
                    .take_while(|DidYouMean(s, _)| *s >= 0.3)
                    .take(3)
                    .map(|DidYouMean(_, v)| v)
                    .enumerate()
                {
                    has_help = true;

                    if i == 0 {
                        b.push("\nDid you mean: ");
                    } else {
                        b.push(", ");
                    }

                    b.push_mono_safer(val);
                }
            },
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
            },
        }

        (b.build(), has_help)
    }

    fn format_cmd_error_with_path(
        &self,
        err: docbot::CommandParseError,
        path: &mut Option<Vec<&'static str>>,
    ) -> (String, bool)
    {
        use docbot::CommandParseError::{
            BadConvert, BadId, MissingRequired, NoInput, Subcommand, Trailing,
        };

        let mut b = MessageBuilder::new();
        let mut has_help = false;
        let mut help_cmd = None;

        match err {
            NoInput => b.push("Expected a command, got nothing"),
            BadId(e) => {
                let (s, help) = Self::format_id_error(e);
                has_help |= help;
                b.push(s)
            },
            MissingRequired(ArgumentName { cmd, arg }) => {
                help_cmd = Some(cmd);
                b.push("Missing required argument ").push_mono_safer(arg)
            },
            BadConvert(ArgumentName { cmd, arg }, err) => {
                enum Downcast {
                    Cmd(docbot::CommandParseError),
                    Id(docbot::IdParseError),
                    Other(anyhow::Error),
                }

                help_cmd = Some(cmd);
                b.push("Failed to process argument ")
                    .push_mono_safer(arg)
                    .push(": ");

                match err.downcast().map_or_else(
                    |e| e.downcast().map_or_else(Downcast::Other, Downcast::Id),
                    Downcast::Cmd,
                ) {
                    Downcast::Cmd(e) => {
                        let (s, help) = self.format_cmd_error_with_path(e, &mut None);
                        has_help |= help;
                        b.push(s)
                    },
                    Downcast::Id(e) => {
                        let (s, help) = Self::format_id_error(e);
                        has_help |= help;
                        b.push(s)
                    },
                    Downcast::Other(e) => b.push_safe(e),
                }
            },
            Trailing(cmd, s) => {
                help_cmd = Some(cmd);
                b.push("Too many arguments given (starting with ")
                    .push_mono_safer(s)
                    .push(")")
            },
            Subcommand(id, err) => {
                if let Some(p) = path.as_mut() {
                    p.push(id)
                }
                let (s, help) = self.format_cmd_error_with_path(*err, path);
                has_help |= help;

                b.push("Subcommand ")
                    .push_mono_safer(id)
                    .push(" failed: ")
                    .push(s)
            },
        };

        if !has_help {
            if let Some(path) = path {
                path.push("help");

                if let Some(cmd) = help_cmd {
                    path.push(cmd);
                }

                b.push("\nRun ")
                    .push_mono_safer(self.prefix_command(path.join(" ")))
                    .push(" for more info");

                has_help = true;
            }
        }

        (b.build(), has_help)
    }

    fn format_cmd_error(&self, err: docbot::CommandParseError) -> String {
        self.format_cmd_error_with_path(err, &mut Some(Vec::new()))
            .0
    }

    async fn handle_role_command(
        &self,
        ctx: Context,
        msg: &Message,
        cmd: RoleCommand,
    ) -> Result<()>
    {
        use roles::{
            RoleCommandError::{GuildRequired, NoPermission, Other},
            RoleCommandOk::{Added, Help, List, Removed, ShowAll, ShowOne},
        };

        let chan = msg.channel_id;

        match roles::execute(cmd, msg.author.id, msg.guild_id, &self.pool, self.superuser) {
            Ok(Help(c)) => self.send_help(chan, ctx, c, "Subcommands").await?,
            Ok(List(r)) => self.send_help(chan, ctx, r, "Roles").await?,
            Ok(ShowAll(_map)) => todo!(),
            Ok(ShowOne(_user, _roles)) => todo!(),
            Ok(Added(n)) => {
                chan.say(
                    &ctx,
                    format!("Added {} role{}.", n, if n == 1 { "" } else { "s" }),
                )
                .await
                .context("failed to send success message")?;
            },
            Ok(Removed(n)) => {
                chan.say(
                    &ctx,
                    format!("Removed {} role{}.", n, if n == 1 { "" } else { "s" }),
                )
                .await
                .context("failed to send success message")?;
            },
            Err(GuildRequired) => Self::send_guild_required(chan, &ctx).await?,
            Err(NoPermission(n)) => Self::send_no_permission(chan, &ctx, n).await?,
            Err(Other(e)) => Err(e).context("an unexpected error occurred")?,
        }

        Ok(())
    }

    async fn handle_channel_command(
        &self,
        ctx: Context,
        msg: &Message,
        cmd: ChannelCommand,
    ) -> Result<()>
    {
        use channels::{
            ChannelCommandError::{GuildRequired, NoPermission, Other},
            ChannelCommandOk::{Help, List, Marked, ShowAll, ShowOne, Unmarked},
        };

        let chan = msg.channel_id;

        match channels::execute(cmd, msg.author.id, msg.guild_id, &self.pool, self.superuser) {
            Ok(Help(c)) => self.send_help(chan, ctx, c, "Subcommands").await?,
            Ok(List(m)) => self.send_help(chan, ctx, m, "Channel modes").await?,
            Ok(ShowAll { .. }) | Ok(ShowOne { .. }) | Ok(Marked) | Ok(Unmarked) => todo!(),
            Err(GuildRequired) => Self::send_guild_required(chan, &ctx).await?,
            Err(NoPermission(n)) => Self::send_no_permission(chan, &ctx, n).await?,
            Err(Other(e)) => Err(e).context("an unexpected error occurred")?,
        }

        Ok(())
    }

    async fn handle_command<S: AsRef<str>>(&self, s: S, ctx: Context, msg: &Message) -> Result<()> {
        use BaseCommand::{Channel, Help, Modmail, Role, Schedule, Version};

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
                chan.say(ctx, format!("**ERROR:** {}", self.format_cmd_error(e)))
                    .await
                    .context("failed to send command parse error")?;
                return Ok(());
            },
        };

        match cmd {
            Help(c) => {
                self.send_help(chan, ctx, BaseCommand::help(c), "Commands")
                    .await?
            },
            Version => Self::send_version(chan, ctx).await?,
            Role(c) => self.handle_role_command(ctx, msg, c).await?,
            Channel(c) => self.handle_channel_command(ctx, msg, c).await?,
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
