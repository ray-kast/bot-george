use crate::{
    bot::{channels, roles},
    commands,
    db::DbPool,
    error::Result,
};
use anyhow::Context as _;
use dispose::defer;
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

    async fn send_guild_required(channel_id: ChannelId, http: impl AsRef<Http>) -> Result<()> {
        channel_id
            .send_message(http, |m| {
                m.content("**ERROR:** This command cannot be used in a DM channel.")
            })
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
            .send_message(http, |m| {
                m.content(format!(
                    "**ERROR:** You do not have permission to {}",
                    match err {
                        Show => "show assigned roles".into(),
                        Add(r) => format!("add the role **{}**", r),
                        Remove(r) => format!("remove the role **{}**", r),
                    }
                ))
            })
            .await
            .context("failed to send error message")?;

        Ok(())
    }

    async fn send_err_message(chan: ChannelId, http: impl AsRef<Http>, err: anyhow::Error) {
        error!("{:?}", err);
        chan.send_message(http, |m| m.content(format!("**ERROR:**\n```{:?}```", err)))
            .await
            .map_err(|e| error!("error while reporting error: {:?}", e))
            .ok();
    }

    async fn handle_command<S: AsRef<str>>(&self, s: S, ctx: Context, msg: &Message) -> Result<()> {
        use crate::commands::BaseCommand::{Channel, Help, Modmail, Role, Schedule};

        let chan = msg.channel_id;
        let http = Arc::clone(&ctx.http);

        let _d = defer(|| {
            if thread::panicking() {
                tokio::task::spawn_blocking(move || {
                    runtime::Builder::new()
                        .enable_all()
                        .basic_scheduler()
                        .max_threads(1)
                        .build()
                        .unwrap()
                        .block_on(chan.send_message(http, |m| {
                            m.content("**ERROR:** thread panicked while servicing your request")
                        }))
                        .ok()
                });
            }
        });

        let cmd = commands::parse_base(s).context("failed to parse command")?;

        match cmd {
            Help(_topic) => todo!(),
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
                    Ok(Help(())) => todo!(),
                    Ok(List(())) => todo!(),
                    Ok(ShowAll(_map)) => todo!(),
                    Ok(ShowOne(_user, _roles)) => todo!(),
                    Ok(Added(n)) => {
                        msg.channel_id
                            .send_message(&ctx, |m| {
                                m.content(format!(
                                    "Added {} role{}.",
                                    n,
                                    if n == 1 { "" } else { "s" }
                                ))
                            })
                            .await
                            .context("failed to send success message")?;
                    },
                    Ok(Removed(n)) => {
                        msg.channel_id
                            .send_message(&ctx, |m| {
                                m.content(format!(
                                    "Removed {} role{}.",
                                    n,
                                    if n == 1 { "" } else { "s" }
                                ))
                            })
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
                    Ok(Help(())) => todo!(),
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
