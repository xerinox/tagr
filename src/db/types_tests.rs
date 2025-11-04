//! Unit tests for database type utilities

#[cfg(test)]
mod tests {
    use crate::db::types::{PathKey, PathString};
    use crate::db::error::DbError;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_path_key_new() {
        let key = PathKey::new("test.txt");
        assert_eq!(key.as_path(), Path::new("test.txt"));
    }

    #[test]
    fn test_path_key_from_pathbuf() {
        let path = PathBuf::from("dir/file.txt");
        let key = PathKey::new(path.clone());
        assert_eq!(key.as_path(), path.as_path());
    }

    #[test]
    fn test_path_key_into_inner() {
        let path = PathBuf::from("test.txt");
        let key = PathKey::new(path.clone());
        let inner = key.into_inner();
        assert_eq!(inner, path);
    }

    #[test]
    fn test_path_key_as_path() {
        let key = PathKey::new("test.txt");
        let path_ref = key.as_path();
        assert_eq!(path_ref, Path::new("test.txt"));
    }

    #[test]
    fn test_path_key_to_vec_u8() {
        let key = PathKey::new("test.txt");
        let result: Result<Vec<u8>, DbError> = key.try_into();
        assert!(result.is_ok());
        
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_path_key_from_bytes() {
        let original_key = PathKey::new("test.txt");
        let bytes: Vec<u8> = original_key.clone().try_into().unwrap();
        
        let restored_key = PathKey::from_bytes(&bytes).unwrap();
        assert_eq!(restored_key.as_path(), Path::new("test.txt"));
    }

    #[test]
    fn test_path_key_roundtrip() {
        let paths = vec![
            "simple.txt",
            "dir/file.txt",
            "deep/nested/path/file.txt",
        ];

        for path_str in paths {
            let original = PathKey::new(path_str);
            let bytes: Vec<u8> = original.clone().try_into().unwrap();
            let restored = PathKey::from_bytes(&bytes).unwrap();
            assert_eq!(original, restored);
        }
    }

    #[test]
    fn test_path_key_equality() {
        let key1 = PathKey::new("test.txt");
        let key2 = PathKey::new("test.txt");
        let key3 = PathKey::new("other.txt");
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_path_key_clone() {
        let key1 = PathKey::new("test.txt");
        let key2 = key1.clone();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_path_string_new() {
        let result = PathString::new("test.txt");
        assert!(result.is_ok());
        
        let path_str = result.unwrap();
        assert_eq!(path_str.as_str(), "test.txt");
    }

    #[test]
    fn test_path_string_from_pathbuf() {
        let path = PathBuf::from("test.txt");
        let result: Result<PathString, DbError> = path.try_into();
        assert!(result.is_ok());
        
        let path_str = result.unwrap();
        assert_eq!(path_str.as_str(), "test.txt");
    }

    #[test]
    fn test_path_string_from_path_ref() {
        let path = Path::new("test.txt");
        let result: Result<PathString, DbError> = path.try_into();
        assert!(result.is_ok());
        
        let path_str = result.unwrap();
        assert_eq!(path_str.as_str(), "test.txt");
    }

    #[test]
    fn test_path_string_as_str() {
        let path_str = PathString::new("dir/file.txt").unwrap();
        assert_eq!(path_str.as_str(), "dir/file.txt");
    }

    #[test]
    fn test_path_string_into_string() {
        let path_str = PathString::new("test.txt").unwrap();
        let string = path_str.into_string();
        assert_eq!(string, "test.txt");
    }

    #[test]
    fn test_path_string_as_ref() {
        let path_str = PathString::new("test.txt").unwrap();
        let str_ref: &str = path_str.as_ref();
        assert_eq!(str_ref, "test.txt");
    }

    #[test]
    fn test_path_string_deref() {
        let path_str = PathString::new("test.txt").unwrap();
        assert_eq!(path_str.len(), 8);
        assert!(path_str.ends_with(".txt"));
    }

    #[test]
    fn test_path_string_equality() {
        let ps1 = PathString::new("test.txt").unwrap();
        let ps2 = PathString::new("test.txt").unwrap();
        let ps3 = PathString::new("other.txt").unwrap();
        
        assert_eq!(ps1, ps2);
        assert_ne!(ps1, ps3);
    }

    #[test]
    fn test_path_string_clone() {
        let ps1 = PathString::new("test.txt").unwrap();
        let ps2 = ps1.clone();
        assert_eq!(ps1, ps2);
    }

    #[test]
    fn test_path_string_with_various_paths() {
        let valid_paths = vec![
            "simple.txt",
            "dir/file.txt",
            "deep/nested/path/file.txt",
            "/absolute/path.txt",
            "./relative/path.txt",
        ];

        for path in valid_paths {
            let result = PathString::new(path);
            assert!(result.is_ok(), "Failed for path: {}", path);
            assert_eq!(result.unwrap().as_str(), path);
        }
    }

    #[test]
    fn test_path_string_debug() {
        let path_str = PathString::new("test.txt").unwrap();
        let debug = format!("{:?}", path_str);
        assert!(debug.contains("PathString"));
        assert!(debug.contains("test.txt"));
    }

    #[test]
    fn test_path_key_debug() {
        let key = PathKey::new("test.txt");
        let debug = format!("{:?}", key);
        assert!(debug.contains("PathKey"));
        assert!(debug.contains("test.txt"));
    }

    #[test]
    fn test_path_string_empty() {
        let result = PathString::new("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "");
    }

    #[test]
    fn test_path_key_empty() {
        let key = PathKey::new("");
        assert_eq!(key.as_path(), Path::new(""));
    }

    #[test]
    fn test_multiple_path_keys_to_bytes() {
        let keys = vec![
            PathKey::new("file1.txt"),
            PathKey::new("file2.txt"),
            PathKey::new("file3.txt"),
        ];

        let bytes_vec: Vec<Vec<u8>> = keys
            .into_iter()
            .map(|k| k.try_into().unwrap())
            .collect();

        assert_ne!(bytes_vec[0], bytes_vec[1]);
        assert_ne!(bytes_vec[1], bytes_vec[2]);
        assert_ne!(bytes_vec[0], bytes_vec[2]);
    }
}
