use crate::TagName;

mod search;
pub mod tag;
pub enum Command {
    SearchCommand { tag: String },
    TagCommand { file: String, tag: TagName },
}
