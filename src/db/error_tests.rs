//! Unit tests for database error types

#[cfg(test)]
mod tests {
    use crate::db::error::DbError;
    use std::error::Error;

    #[test]
    fn test_file_not_found_error() {
        let error = DbError::FileNotFound("test.txt".to_string());
        assert_eq!(error.to_string(), "File not found: test.txt");
    }

    #[test]
    fn test_serialize_error() {
        let error = DbError::SerializeError("Invalid UTF-8".to_string());
        assert_eq!(error.to_string(), "Error during serialization: Invalid UTF-8");
    }

    #[test]
    fn test_error_display() {
        let error = DbError::FileNotFound("/path/to/file.txt".to_string());
        let display = format!("{}", error);
        assert!(display.contains("File not found"));
        assert!(display.contains("/path/to/file.txt"));
    }

    #[test]
    fn test_error_debug() {
        let error = DbError::SerializeError("test error".to_string());
        let debug = format!("{:?}", error);
        assert!(debug.contains("SerializeError"));
        assert!(debug.contains("test error"));
    }

    #[test]
    fn test_error_source() {
        let error = DbError::FileNotFound("test.txt".to_string());
        assert!(error.source().is_none());
    }

    #[test]
    fn test_serialize_error_creation() {
        let msg = "UTF-8 conversion failed";
        let error = DbError::SerializeError(msg.to_string());
        
        match error {
            DbError::SerializeError(s) => assert_eq!(s, msg),
            _ => panic!("Expected SerializeError variant"),
        }
    }

    #[test]
    fn test_file_not_found_error_creation() {
        let path = "missing_file.txt";
        let error = DbError::FileNotFound(path.to_string());
        
        match error {
            DbError::FileNotFound(p) => assert_eq!(p, path),
            _ => panic!("Expected FileNotFound variant"),
        }
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DbError>();
    }

    #[test]
    fn test_error_pattern_matching() {
        let errors = vec![
            DbError::FileNotFound("file1.txt".to_string()),
            DbError::SerializeError("error1".to_string()),
        ];

        for error in errors {
            match error {
                DbError::FileNotFound(path) => {
                    assert_eq!(path, "file1.txt");
                }
                DbError::SerializeError(msg) => {
                    assert_eq!(msg, "error1");
                }
                _ => {}
            }
        }
    }
}
