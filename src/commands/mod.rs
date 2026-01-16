//! Command implementations
//!
//! Each command is a module with an execute function that takes parsed CLI args
//! and executes the operation against the database.

pub mod alias;
pub mod browse;
pub mod bulk;
pub mod cleanup;
pub mod filter;
pub mod list;
pub mod note;
pub mod search;
pub mod tag;
pub mod tags;

// Re-export execute functions for convenience
pub use alias::execute_alias_command as alias;
pub use browse::execute as browse;
pub use cleanup::execute as cleanup;
pub use filter::execute as filter;
pub use list::execute as list;
pub use search::execute as search;
pub use tag::execute as tag;
pub use tags::execute as tags;
