table! {
    channel_modes (channel_id, mode) {
        channel_id -> Uuid,
        mode -> Text,
    }
}

table! {
    channels (id) {
        id -> Uuid,
        alias -> Varchar,
        channel_id -> Int8,
    }
}

table! {
    default_channel_modes (guild_id) {
        guild_id -> Int8,
        mode -> Nullable<Text>,
    }
}

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

joinable!(channel_modes -> channels (channel_id));
joinable!(user_roles -> users (user_id));

allow_tables_to_appear_in_same_query!(
    channel_modes,
    channels,
    default_channel_modes,
    user_roles,
    users,
);
