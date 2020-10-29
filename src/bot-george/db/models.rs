use crate::schema::{user_roles, users};
use diesel::Queryable;
use serenity::model::id::UserId;
use std::{
    cmp::PartialEq,
    hash::{Hash, Hasher},
};
use uuid::Uuid;

///// Users

#[derive(Queryable, Eq)]
pub struct User {
    pub id: Uuid,
    pub alias: String,
}

pub struct DisplayUser {
    pub user_id: UserId,
    pub alias: String,
}

#[derive(Insertable, Debug)]
#[table_name = "users"]
pub struct NewUser {
    pub id: Uuid,
    pub alias: String,
    pub user_id: i64,
    pub guild_id: i64,
}

#[derive(Insertable, Debug)]
#[table_name = "user_roles"]
pub struct NewUserRole {
    pub user_id: Uuid,
    pub role: String,
}

impl PartialEq for User {
    fn eq(&self, rhs: &User) -> bool { self.id == rhs.id }
}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}

///// Channels

#[derive(Queryable, Eq)]
pub struct Channel {
    pub id: Uuid,
    pub alias: String,
}

impl PartialEq for Channel {
    fn eq(&self, rhs: &Channel) -> bool { self.id == rhs.id }
}

impl Hash for Channel {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
