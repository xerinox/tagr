pub mod cache;
pub mod config;
pub mod evaluator;
pub mod parser;
pub mod types;

pub use cache::{FileMetadata, MetadataCache};
pub use config::VirtualTagConfig;
pub use evaluator::VirtualTagEvaluator;
pub use parser::{ParseError, VirtualTagParser};
pub use types::{
    ExtTypeCategory, GitCondition, PermissionCondition, RangeCondition, SizeCategory,
    SizeCondition, TimeCondition, VirtualTag,
};
