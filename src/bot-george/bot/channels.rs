use super::roles::NoPermissionError;
use crate::{
    db::{models::Channel, DbPool},
    error::Result,
};
use anyhow::Context;
use diesel::{prelude::*, result::Error as DieselError};
use docbot::Docbot;
use serenity::model::id::{ChannelId, GuildId, UserId};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Docbot, Debug)]
/// TODO
pub enum ChannelCommand {
    /// help [command]
    /// Get help with managing channel behavior, or a particular channel subcommand
    Help(Option<ChannelCommandId>),

    /// (list|ls)
    /// List the available channel modes
    List,

    /// show [channel]
    /// Show all channel modes, or list the mode of a given channel
    Show(Option<ChannelId>),

    /// default <mode>
    /// Set the default behavior mode for unmarked channels
    Default(ChannelMode),

    /// (mark|set) <channel> <mode>
    /// Change the behavior of the bot for a specific channel
    Mark(ChannelId, ChannelMode),

    /// (unmark|clear|reset) <channel>
    /// Clear any channel-specific behavior for a channel, resetting it to the default
    Unmark(ChannelId),
}

/// TODO: remove this
#[derive(Docbot, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelMode {
    /// (disabled|none)
    Disabled,
    /// (announcements|broadcast)
    Announcements,
    /// (commands|command-only)
    Commands,
}

pub type ChannelCommandResult<T> = Result<T, ChannelCommandError>;

pub enum ChannelCommandOk {
    Help(()),
    List(()),
    ShowAll {
        default: ChannelMode,
        modes: HashMap<Channel, ChannelMode>,
    },
    ShowOne {
        is_default: bool,
        channel: Channel,
        mode: ChannelMode,
    },
    Marked,
    Unmarked,
}

#[derive(Error, Debug)]
pub enum ChannelCommandError {
    #[error("no guild ID was provided")]
    GuildRequired,
    #[error("{0}")]
    NoPermission(#[from] NoPermissionError),
    #[error("an unexpected error occurred")]
    Other(#[from] anyhow::Error),
}

pub fn get_channel(channel: ChannelId, db: &DbPool) -> Result<Option<Channel>> {
    use crate::schema::channels::dsl::{alias, channel_id, channels, id};

    let db_conn = db.get().context("failed to connect to the database")?;

    #[allow(clippy::cast_possible_wrap)]
    match channels
        .filter(channel_id.eq(channel.0 as i64))
        .select((id, alias))
        .first::<Channel>(&db_conn)
    {
        Ok(r) => Ok(Some(r)),
        Err(DieselError::NotFound) => Ok(None),
        Err(e) => Err(e).context("failed to retrieve channel from database"),
    }
}

pub fn execute(
    command: ChannelCommand,
    sender: UserId,
    guild: Option<GuildId>,
    db: &DbPool,
    superuser: UserId,
) -> ChannelCommandResult<ChannelCommandOk>
{
    let get_guild = || guild.ok_or(ChannelCommandError::GuildRequired);

    Ok(match command {
        ChannelCommand::Help(_topic) => ChannelCommandOk::Help(todo!()),
        ChannelCommand::List => ChannelCommandOk::List(todo!()),
        ChannelCommand::Show(_target) => todo!(),
        ChannelCommand::Default(_mode) => todo!(),
        ChannelCommand::Mark(_target, _mode) => todo!(),
        ChannelCommand::Unmark(_target) => todo!(),
    })
}
