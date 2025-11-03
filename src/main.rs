//! Tagr, a tool to tag files
use std::{path::PathBuf, string::FromUtf8Error};
use sled::IVec;
use serde::{Serialize, Deserialize};
use bincode::{self, Decode, Encode, error::{DecodeError, EncodeError}};
use thiserror::Error;

/// Error enum, contains all failure states of the program
#[derive(Debug, Error)]
enum TagrError {
    /// Represents a bincode decoding error
    #[error("Error while decoding pairing {0}")]
    TagrDecodeError(#[from] DecodeError),
    /// Represents a bincode encoding error
    #[error("Error while encoding pairing {0}")]
    TagrEncodeError(#[from] EncodeError),
    /// Not yet implemented
    #[error("Error during serialization {0}")]
    SerializeError(String),
    /// Represents failure at decoding from utf8
    #[error("Error while creating string from Utf8 {0}")]
    TagrUtf8Error(#[from] FromUtf8Error),
}

/// Data struct containing the pairings of file and tags
#[derive(Encode, Decode, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct Pair {
    file: PathBuf,
    tags: Vec<String>,
}

impl TryInto<(Vec<u8>, IVec)> for Pair {
    type Error = TagrError;
    fn try_into(self) -> Result<(Vec<u8>, IVec), Self::Error> {
        let serialized_tags = bincode::encode_to_vec(self.tags, bincode::config::standard())?;
        let serialized_file = self.file.into_os_string().into_string().unwrap().as_bytes().to_owned();
        Ok((serialized_file, IVec::from(serialized_tags)))
    }
}

impl TryFrom<(&[u8], IVec)> for Pair  {
    type Error = TagrError;
    fn try_from(bytes: (&[u8], IVec)) -> Result<Self, Self::Error> {
        let vec_of_u8 = bytes.0.to_owned();
        let file: PathBuf = String::from_utf8(vec_of_u8)?.try_into().expect("Should be able to convert into a path");

        let (tags, _len): (Vec<String>, usize) = bincode::decode_from_slice(&bytes.1, bincode::config::standard())?;
        Ok(Pair {
            file, tags
        })
    }
}

fn main() {
    println!("Hello, world!");
    let db = sled::open("my_db").unwrap();
    let p = Pair {file: PathBuf::new(), tags: vec!["test_one".into(), "test_two".into()]};
    let iv: (Vec<u8>, IVec) = p.try_into().unwrap();
    db.insert(iv.0, iv.1);
    let result = db.get(PathBuf::new().into_os_string().into_string().unwrap().as_bytes().to_owned()).unwrap().unwrap();
    let pair: Result<Pair, _> = (PathBuf::new().into_os_string().into_string().unwrap().as_bytes(), result).try_into();
    
    dbg!(pair);
}
