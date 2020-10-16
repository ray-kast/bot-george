//! Contains the bot command definitions

use crate::error::Result;
use docbot::{prelude::*, CommandParseError};
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Docbot, Debug)]
/// TODO
pub enum BaseCommand {
    /// help [command]
    /// Display information about the bot, or get help on a particular command
    Help(Option<String>),

    /// (role|roles) <subcommand...>
    /// Manage bot-specific roles for users
    Role(#[docbot(subcommand)] RoleCommand),

    /// (modmail|mm) <message...>
    /// Send a message to the moderators without any personal data attached
    Modmail(Vec<String>),
}

#[derive(Docbot, Debug)]
/// TODO
pub enum RoleCommand {
    /// help [command]
    /// Get help with managing roles, or a particular role subcommand
    Help(Option<String>),

    /// (list|ls)
    /// List the available roles
    List,

    /// show [user]
    /// Show all assigned roles, or list the roles of a given user
    Show(Option<String>),

    /// add <user> <roles...>
    /// Add one or more roles to a user
    Add(String, Vec<String>),

    /// (remove|rm) <user> <roles...>
    /// Remove one or more roles from a user
    Remove(String, Vec<String>),
}

lazy_static! {
    static ref COMMAND_ARG_RE: Regex =
        Regex::new(r#"\s*(?:([^'"]\S*)|'([^']*)'|"((?:[^"\\]|\\.)*)")"#).unwrap();
    static ref COMMAND_DQUOTE_ESCAPE_RE: Regex = Regex::new(r"\\(.)").unwrap();
}

/// Parse a base command from a string
pub fn parse_base<S: AsRef<str>>(s: S) -> Result<BaseCommand, CommandParseError> {
    let toks = COMMAND_ARG_RE.captures_iter(s.as_ref()).map(|cap| {
        if let Some(dquot) = cap.get(3) {
            COMMAND_DQUOTE_ESCAPE_RE.replace_all(dquot.as_str(), "$1")
        } else if let Some(squot) = cap.get(2) {
            squot.as_str().into()
        } else {
            cap.get(1).unwrap().as_str().into()
        }
    });

    BaseCommand::parse(toks)
}
