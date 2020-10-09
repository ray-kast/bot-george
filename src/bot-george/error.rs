//! Types and utilities for error handling

// In the event this becomes a sum type at some point
/// Unified error type for the program
pub type Error = anyhow::Error;

/// Result type alias, defaulting to the crate Error type
pub type Result<T, E = Error> = std::result::Result<T, E>;
