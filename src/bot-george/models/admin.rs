use crate::error::{Error, Result};
use anyhow::anyhow;
use std::str::FromStr;

/// The permission level of an admin.
///
/// Greater values correspond with more permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdminRole {
    /// Grants control of the bot commands
    Admin,
    /// The owner of the bot.
    ///
    /// This role is hard-configured and should not be convertible to or from a string.
    Superuser,
}

impl FromStr for AdminRole {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "admin" => Self::Admin,
            s => return Err(anyhow!("invalid admin role {:?}", s)),
        })
    }
}

/// An administrator user, with a given permission level
#[derive(Queryable, Debug)]
pub struct Admin {
    user_id: i64,
    role: String,
}

impl Admin {
    /// The snowflake ID of this user
    pub fn user_id(&self) -> u64 { self.user_id as u64 }

    /// The role of this user, parsed from a string
    pub fn role(&self) -> Result<AdminRole> { self.role.parse() }
}
