CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
  id         uuid PRIMARY KEY NOT NULL DEFAULT uuid_generate_v4(),
  alias      varchar(128) NOT NULL,
  user_id    bigint NOT NULL,
  guild_id   bigint NOT NULL,

  UNIQUE(user_id, guild_id)
);
