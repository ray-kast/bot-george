//! Contains the bot command definitions

use crate::{
    bot::{channels::ChannelCommand, roles::RoleCommand, schedule::ScheduleCommand},
    error::Result,
};
use docbot::{prelude::*, CommandParseError};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Docbot, Debug)]
/// TODO
pub enum BaseCommand {
    /// help [command]
    /// Display information about the bot, or get help on a particular command
    Help(Option<BaseCommandId>),

    /// (role|roles) <subcommand...>
    /// Manage bot-specific roles for users
    Role(#[docbot(subcommand)] RoleCommand),

    /// channel <subcommand...>
    /// Manage channel-specific bot behavior
    Channel(#[docbot(subcommand)] ChannelCommand),

    /// schedule <subcommand...>
    /// Manage scheduled announcements
    Schedule(#[docbot(subcommand)] ScheduleCommand),

    /// (modmail|mm) <message...>
    /// Send a message to the moderators without any personal data attached
    Modmail(Vec<String>),
}

lazy_static! {
    static ref COMMAND_ARG_RE: Regex =
        Regex::new(r#"\s*(?:([^'"]\S*)|'([^']*)'|"((?:[^"\\]|\\.)*)")"#).unwrap();
    static ref COMMAND_DQUOTE_ESCAPE_RE: Regex = Regex::new(r"\\(.)").unwrap();
    static ref USER_MENTION_RE: Regex = Regex::new(r"^\s*<@!(\d+)>\s*$").unwrap();
}

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
