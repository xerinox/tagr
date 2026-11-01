use std::str::FromStr;

use crate::Command;
use crate::types::TagrPath;
use crate::store::{self, Connected, StoreError};

///# Errors
pub async fn run(cmd: Command, store: &store::Store<Connected>) -> Result<(), StoreError> {
    todo!();
    /*
    #[allow(clippy::match_wildcard_for_single_variants)]
    match cmd {
        Command::TagCommand { file, tag } => store.tag_file(&[&tag], &TagrPath::from_str(&file).unwrap()).await,
        _ => Err(StoreError::MismatchedCommand),
    }
    */
}
