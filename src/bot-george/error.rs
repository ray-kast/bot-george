// In the event this becomes a sum type at some point
pub type Error = anyhow::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;
