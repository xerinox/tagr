//! Command implementations
//! 
//! Each command is a module with an execute function that takes parsed CLI args
//! and executes the operation against the database.

pub mod browse;
pub mod cleanup;
pub mod list;
pub mod search;
pub mod tag;
pub mod tags;

// Re-export execute functions for convenience
pub use browse::execute as browse;
pub use cleanup::execute as cleanup;
pub use list::execute as list;
pub use search::execute as search;
pub use tag::execute as tag;
pub use tags::execute as tags;
