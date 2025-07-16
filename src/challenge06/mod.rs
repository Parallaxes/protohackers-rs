pub mod protocol;
pub mod client;
pub mod db;
pub mod challenge06;

#[cfg(test)]
mod tests;

// Re-exports for convenience
pub use protocol::{Message, ParseError, parse};
pub use client::{Client, ClientType};
pub use db::Database;