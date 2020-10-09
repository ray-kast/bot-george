//! Contains the bot command definitions

use docbot::Docbot;

// TODO: fix false-positive ambiguities (e.g. 'm')

#[derive(Docbot, Debug)]
/// TODO
pub enum BaseCommand<'a> {
    /// help [command]
    /// Display information about the bot, or get help on a particular command
    Help(Option<&'a str>),

    /// (role|roles) <subcommand...>
    /// Manage bot-specific roles for users
    Role(Vec<&'a str>),

    /// (modmail|mm) <message...>
    /// Send a message to the moderators without any personal data attached
    Modmail(Vec<&'a str>),
}

#[derive(Docbot, Debug)]
/// TODO
pub enum RoleCommand<'a> {
    /// help [command]
    /// Get help with managing roles, or a particular role subcommand
    Help(Option<&'a str>),

    /// (list|ls)
    /// List the available roles
    List,

    /// show [user]
    /// Show all assigned roles, or list the roles of a given user
    Show(Option<&'a str>),

    /// add <user> <role>
    /// Add a role to a user
    Add(&'a str, &'a str),

    /// (remove|rm) <user> <role>
    /// Remove a role from a user
    Remove(&'a str, &'a str),
}
