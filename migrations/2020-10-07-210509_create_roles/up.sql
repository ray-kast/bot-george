CREATE TABLE user_roles (
  user_id uuid NOT NULL REFERENCES users(id),
  role    text,

  PRIMARY KEY(user_id, role)
);

CREATE INDEX ON user_roles(user_id);
