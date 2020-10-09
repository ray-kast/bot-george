table! {
    user_roles (user_id, role) {
        user_id -> Uuid,
        role -> Text,
    }
}

table! {
    users (id) {
        id -> Uuid,
        alias -> Varchar,
        user_id -> Int8,
        guild_id -> Int8,
    }
}

joinable!(user_roles -> users (user_id));

allow_tables_to_appear_in_same_query!(user_roles, users,);
