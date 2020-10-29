use crate::{
    db::{
        models::{DisplayUser, NewUser, NewUserRole, User},
        DbPool,
    },
    error::Result,
};
use anyhow::Context;
use diesel::{prelude::*, result::Error as DieselError};
use docbot::Docbot;
use log::{error, warn};
use serenity::model::id::{GuildId, UserId};
use std::{
    collections::{BTreeSet, HashMap},
    ops::Bound,
};
use thiserror::Error;
use uuid::Uuid;

// TODO: allow referring to users by bot-assigned alias?
#[derive(Docbot, Debug)]
/// TODO
pub enum RoleCommand {
    /// help [command]
    /// Get help with managing roles, or a particular role subcommand
    Help(Option<RoleCommandId>),

    /// (list|ls)
    /// List the available roles
    List,

    /// show [user]
    /// Show all assigned roles, or list the roles of a given user
    Show(Option<UserId>),

    /// add <user> <roles...>
    /// Add one or more roles to a user
    Add(UserId, BTreeSet<Role>),

    /// (remove|rm) <user> <roles...>
    /// Remove one or more roles from a user
    Remove(UserId, BTreeSet<Role>),
}

/// TODO: remove this
#[derive(Docbot, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// admin
    Admin,
    /// (mod|moderator)
    Mod,
}

pub type RoleCommandResult<T> = Result<T, RoleCommandError>;

pub enum RoleCommandOk {
    Help(()),
    List(()),
    ShowAll(HashMap<DisplayUser, BTreeSet<Role>>),
    ShowOne(DisplayUser, BTreeSet<Role>),
    Added(usize),
    Removed(usize),
}

#[derive(Error, Debug)]
pub enum RoleCommandError {
    #[error("no guild ID was provided")]
    GuildRequired,
    #[error("{0}")]
    NoPermission(#[from] NoPermissionError),
    #[error("an unexpected error occurred")]
    Other(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum NoPermissionError {
    #[error("missing permissions to show assigned roles")]
    Show,
    #[error("missing permissions to add role {0:?}")]
    Add(Role),
    #[error("missing permissions to remove role {0:?}")]
    Remove(Role),
}

pub fn get_user(user: UserId, guild: GuildId, db: &DbPool) -> Result<Option<User>> {
    use crate::schema::users::dsl::{alias, guild_id, id, user_id, users};

    let db_conn = db.get().context("failed to connect to the database")?;

    #[allow(clippy::cast_possible_wrap)]
    match users
        .filter(user_id.eq(user.0 as i64).and(guild_id.eq(guild.0 as i64)))
        .select((id, alias))
        .first::<User>(&db_conn)
    {
        Ok(r) => Ok(Some(r)),
        Err(DieselError::NotFound) => Ok(None),
        Err(e) => Err(e).context("failed to retrieve user from database"),
    }
}

fn add_user(
    user: UserId,
    guild: GuildId,
    user_alias: impl Into<String>,
    db: &DbPool,
) -> Result<User>
{
    use crate::schema::users::dsl::users;

    let db_conn = db.get().context("failed to connect to the database")?;
    let uuid = Uuid::new_v4();
    let user_alias = user_alias.into();

    #[allow(clippy::cast_possible_wrap)]
    diesel::insert_into(users)
        .values(vec![NewUser {
            id: uuid,
            user_id: user.0 as i64,
            guild_id: guild.0 as i64,
            alias: user_alias.clone(),
        }])
        .execute(&db_conn)
        .context("failed to insert new user")?;

    Ok(User {
        id: uuid,
        alias: user_alias,
    })
}

fn get_roles(user: &User, db: &DbPool) -> Result<BTreeSet<Role>> {
    use crate::schema::user_roles::dsl::{role, user_id, user_roles};

    let db_conn = db.get().context("failed to connect to the database")?;

    let mut remove = BTreeSet::new();

    let roles = user_roles
        .filter(user_id.eq(user.id))
        .select(role)
        .load::<String>(&db_conn)
        .context("failed to retrieve user roles from database")?
        .into_iter()
        .filter_map(|r| {
            r.parse()
                .map_err(|e| {
                    warn!("role {:?} couldn't be parsed: {:?}", r, e);
                    remove.insert(r);
                })
                .ok()
        })
        .collect();

    if !remove.is_empty() {
        warn!("Removing invalid roles off {:?}: {:?}", user.alias, remove);

        diesel::delete(user_roles.filter(user_id.eq(user.id).and(role.eq_any(remove))))
            .execute(&db_conn)
            .context("failed to remove broken roles")?;
    }

    Ok(roles)
}

fn insert_roles(user: &User, roles: BTreeSet<Role>, db: &DbPool) -> Result<()> {
    use crate::schema::user_roles::dsl::user_roles;

    let db_conn = db.get().context("failed to connect to the database")?;

    diesel::insert_into(user_roles)
        .values(
            roles
                .into_iter()
                .map(|r| NewUserRole {
                    user_id: user.id,
                    role: format!("{}", r),
                })
                .collect::<Vec<_>>(),
        )
        .execute(&db_conn)
        .context("failed to insert new roles")?;

    Ok(())
}

fn delete_roles(user: &User, roles: BTreeSet<Role>, db: &DbPool) -> Result<()> {
    use crate::schema::user_roles::dsl::{role, user_id, user_roles};

    let db_conn = db.get().context("failed to connect to the database")?;

    diesel::delete(
        user_roles.filter(
            user_id
                .eq(user.id)
                .and(role.eq_any(roles.into_iter().map(|r| format!("{}", r)))),
        ),
    )
    .execute(&db_conn)
    .context("failed to delete roles")?;

    // TODO: make this a postgres hook?
    if !diesel::select(diesel::dsl::exists(user_roles.filter(user_id.eq(user.id))))
        .get_result(&db_conn)
        .context("failed to query for remaining roles")?
    {
        use crate::schema::users::dsl::{id, users};

        diesel::delete(users.filter(id.eq(user.id)))
            .execute(&db_conn)
            .context("failed to delete orphaned user")?;
    }

    Ok(())
}

pub fn execute(
    command: RoleCommand,
    sender: UserId,
    guild: Option<GuildId>,
    db: &DbPool,
    superuser: UserId,
) -> RoleCommandResult<RoleCommandOk>
{
    let is_super = sender == superuser;

    let get_guild = || guild.ok_or(RoleCommandError::GuildRequired);

    let get_sender = |guild| -> RoleCommandResult<_> {
        let sender = get_user(sender, guild, db).context("failed to get sender")?;
        let sender_roles = sender
            .as_ref()
            .map_or_else(|| Ok(BTreeSet::new()), |u| get_roles(u, db))
            .context("failed to get sender permissions")?;

        Ok((sender, sender_roles))
    };

    let get_target = |target, guild| -> RoleCommandResult<_> {
        let target = get_user(target, guild, db).context("failed to get target")?;
        let target_roles = target
            .as_ref()
            .map_or_else(|| Ok(BTreeSet::new()), |u| get_roles(u, db))
            .context("failed to get target permissions")?;

        Ok((target, target_roles))
    };

    Ok(match command {
        RoleCommand::Help(_topic) => RoleCommandOk::Help(todo!()),
        RoleCommand::List => RoleCommandOk::List(todo!()),
        RoleCommand::Show(target) => {
            let guild = get_guild()?;
            let (_, sender_roles) = get_sender(guild)?;

            if sender_roles.is_empty() {
                return Err(NoPermissionError::Show.into());
            }

            match target {
                Some(t) => {
                    let (target, target_roles) = get_target(t, guild)?;

                    RoleCommandOk::ShowOne(
                        DisplayUser {
                            alias: target.map_or_else(|| "???".into(), |t| t.alias),
                            user_id: t,
                        },
                        target_roles,
                    )
                },
                None => RoleCommandOk::ShowAll(todo!()),
            }
        },
        RoleCommand::Add(target_id, roles) => {
            let guild = get_guild()?;
            let (_, sender_roles) = get_sender(guild)?;
            let (target, target_roles) = get_target(target_id, guild)?;

            if !is_super {
                if let Some(role) = roles
                    .range((
                        sender_roles
                            .iter()
                            .next_back()
                            .map_or(Bound::Unbounded, Bound::Included),
                        Bound::Unbounded,
                    ))
                    .next()
                {
                    return Err(NoPermissionError::Add(*role).into());
                }
            }

            let to_add: BTreeSet<_> = roles.difference(&target_roles).copied().collect();
            let num = to_add.len();

            let target = target
                .map_or_else(|| add_user(target_id, guild, "???", db), Ok)
                .context("failed to add new user entry for target")?;

            insert_roles(&target, to_add, db).context("failed to add roles to target")?;

            RoleCommandOk::Added(num)
        },
        RoleCommand::Remove(target, roles) => {
            let guild = get_guild()?;
            let (_, sender_roles) = get_sender(guild)?;
            let (target, target_roles) = get_target(target, guild)?;

            if !is_super {
                if let Some(role) = roles
                    .range((
                        sender_roles
                            .iter()
                            .next_back()
                            .map_or(Bound::Unbounded, Bound::Included),
                        Bound::Unbounded,
                    ))
                    .next()
                {
                    return Err(NoPermissionError::Remove(*role).into());
                }
            }

            if let Some(target) = target {
                let to_remove: BTreeSet<_> = roles.intersection(&target_roles).copied().collect();
                let num = to_remove.len();

                delete_roles(&target, to_remove, db)
                    .context("failed to remove roles from target")?;

                RoleCommandOk::Removed(num)
            } else {
                RoleCommandOk::Removed(0)
            }
        },
    })
}
