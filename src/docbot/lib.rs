#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

//! Create a chatbot command interface using a docopt-like API

use std::convert::Infallible;
use thiserror::Error;

/// Error type for failures when parsing a command ID
#[derive(Error, Debug)]
pub enum IdParseError {
    /// No IDs matched the given string
    #[error("no ID match for {0:?}")]
    NoMatch(String),
    /// Multiple IDs could match the given string
    ///
    /// Usually a result of specifying too few characters
    #[error("ambiguous ID {0:?}, could be any of {}", .0.join(", "))]
    Ambiguous(&'static [&'static str], String),
}

/// Error type for failures when parsing a command
#[derive(Error, Debug)]
pub enum CommandParseError {
    /// The iterator returned None immediately
    #[error("no values in command parse input")]
    NoInput,
    /// The command ID could not be parsed
    #[error("failed to parse command ID")]
    BadId(#[from] IdParseError),
    /// A required argument was missing
    #[error("missing required argument {0:?}")]
    MissingRequired(&'static str),
    /// `TryFrom::try_from` failed for an argument
    #[error("failed to convert argument {0:?} from a string")]
    BadConvert(&'static str, anyhow::Error),
    /// Extra arguments were provided
    #[error("trailing argument {0:?}")]
    Trailing(String),
    /// A subcommand failed to parse
    #[error("failed to parse subcommand")]
    Subcommand(Box<CommandParseError>),
}

impl From<Infallible> for CommandParseError {
    fn from(_: Infallible) -> CommandParseError { unreachable!() }
}

pub use anyhow::Error as Anyhow;
pub use docbot_derive::*;

/// A parsable, identifiable command or family of commands
pub trait Command: Sized {
    /// The type of the command ID
    type Id;

    /// Try to parse a sequence of arguments as a command
    /// # Errors
    /// Should return an error for syntax or command-not-found errors, or for
    /// any errors while parsing arguments.
    fn parse<I: IntoIterator<Item = S>, S: AsRef<str>>(iter: I) -> Result<Self, CommandParseError>;

    /// Return an ID uniquely describing the base type of this command.
    fn id(&self) -> Self::Id;
}

/// Usage description for an argument
#[derive(Debug, Clone)]
pub struct ArgumentUsage {
    /// The name of the argument
    pub name: &'static str,
    /// Whether the argument is required
    pub is_required: bool,
    /// Whether the argument is a rest parameter
    pub is_rest: bool,
}

/// Usage description for a command
#[derive(Debug, Clone)]
pub struct CommandUsage {
    /// The possible IDs of this command
    pub ids: &'static [&'static str],
    /// Usage descriptions for this command's arguments
    pub args: &'static [ArgumentUsage],
    /// A short description
    pub desc: &'static str,
}

/// Detailed description of a command
#[derive(Debug, Clone)]
pub struct CommandDesc {
    /// A detailed summary of the command's behavior
    pub summary: &'static str,
    /// Descriptions of the command's arguments
    pub args: &'static [(&'static str, &'static str)],
    /// Example uses of the command
    pub examples: Option<&'static str>,
}

/// A generic help topic
#[derive(Debug, Clone)]
pub enum HelpTopic {
    /// A help topic referring to a single command
    Command(CommandUsage, CommandDesc),
    /// A help topic referring to a set of commands, prefaced by a summary
    CommandSet(&'static str, &'static [CommandUsage]),
    /// A custom help topic
    Custom(&'static str),
}

/// A command with associated help topics
pub trait Help: Command {
    /// Retrieve the help topic corresponding to the given ID.
    fn help(topic: Option<Self::Id>) -> &'static HelpTopic;
}

/// Common traits and types used with this crate
pub mod prelude {
    pub use super::{Command, Docbot, Help};
}
