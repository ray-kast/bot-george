CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE channels (
  id uuid    PRIMARY KEY NOT NULL DEFAULT uuid_generate_v4(),
  alias      varchar(128) NOT NULL,
  channel_id bigint NOT NULL,

  UNIQUE(channel_id)
);

CREATE TABLE channel_modes (
  channel_id uuid NOT NULL REFERENCES channels(id),
  mode       text,

  PRIMARY KEY(channel_id, mode)
);

CREATE INDEX ON channel_modes(channel_id);

CREATE TABLE default_channel_modes (
  guild_id bigint PRIMARY KEY NOT NULL,
  mode     text
);
