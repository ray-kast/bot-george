//! Contains the bot command definitions

use crate::{
    bot::{channels::ChannelCommand, roles::RoleCommand, schedule::ScheduleCommand},
    error::Result,
};
use docbot::{prelude::*, CommandParseError};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Docbot, Debug)]
/// TODO: document `BaseCommand`
/// TODO: prefix command names
pub enum BaseCommand {
    /// help [command]
    /// Display information about the bot, or get help on a particular command
    ///
    /// # Arguments
    /// command: The name of a command to get info for
    Help(Option<BaseCommandId>),

    /// version
    /// Display the bot version and build info
    Version,

    /// (role|roles) <subcommand...>
    /// Manage bot-specific roles for users
    ///
    /// # Arguments
    /// subcommand: The subcommand to run.  Run [`roles help`]() for more info
    Role(#[docbot(subcommand)] RoleCommand),

    /// channel <subcommand...>
    /// Manage channel-specific bot behavior
    ///
    /// # Arguments
    /// subcommand: The subcommand to run.  Run [`channel help`]() for more info
    Channel(#[docbot(subcommand)] ChannelCommand),

    /// schedule <subcommand...>
    /// Manage scheduled announcements
    ///
    /// # Arguments
    /// subcommand: The subcommand to run.  Run [`schedule help`]() for more
    ///             info
    Schedule(#[docbot(subcommand)] ScheduleCommand),

    /// (modmail|mm) <message...>
    /// Send a message to the moderators without any personal data attached
    ///
    /// # Overview
    /// Send a message to the server moderators, where it can be read and
    /// replied to without any information identifying you being passed along by
    /// the bot.  **Note that Discord is not a secure medium.  We cannot take
    /// responsibility for how Discord handles your data.**
    ///
    /// # Arguments
    /// message: The contents of the message to send.  Only this and an
    ///          anonymous ticket ID will be displayed in the sent message.
    Modmail(Vec<String>),
}

lazy_static! {
    static ref COMMAND_ARG_RE: Regex =
        Regex::new(r#"\s*(?:([^'"]\S*)|'([^']*)'|"((?:[^"\\]|\\.)*)")"#).unwrap();
    static ref COMMAND_DQUOTE_ESCAPE_RE: Regex = Regex::new(r"\\(.)").unwrap();
    static ref USER_MENTION_RE: Regex = Regex::new(r"^\s*<@!(\d+)>\s*$").unwrap();
}

// TODO: make command matching case-insensitive?
/// Parse a base command from a string
/// # Errors
/// Returns an error if the command parser failed to find a matching command for
/// the given strings, if a syntax error occurred while parsing the command, or
/// if parsing any arguments returned an error.
pub fn parse_base<S: AsRef<str>>(s: S) -> Result<BaseCommand, CommandParseError> {
    let tokens = COMMAND_ARG_RE.captures_iter(s.as_ref()).map(|cap| {
        cap.get(3).map_or_else(
            || {
                cap.get(2)
                    .unwrap_or_else(|| cap.get(1).unwrap())
                    .as_str()
                    .into()
            },
            |dquote| COMMAND_DQUOTE_ESCAPE_RE.replace_all(dquote.as_str(), "$1"),
        )
    });

    BaseCommand::parse(tokens)
}
