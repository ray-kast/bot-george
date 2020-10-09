#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]

//! Create a chatbot command interface using a docopt-like API

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
    /// The command arguments could not be parsed
    #[error("failed to parse command arguments: {0}")]
    BadArgs(&'static str),
}

pub use docbot_derive::*;

/// A parseable, identifiable command or family of commands
pub trait Command: Sized {
    /// The type of the command ID
    type Id;

    /// Try to parse a sequence of arguments as a command
    fn parse<I: IntoIterator<Item = S>, S: AsRef<str>>(iter: I) -> Result<Self, CommandParseError>;

    /// Return an ID uniquely describing the base type of this command.
    fn id(&self) -> Self::Id;
}

/// Common traits and types used with this crate
pub mod prelude {
    pub use super::Command;
}
