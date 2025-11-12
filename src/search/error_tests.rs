//! Unit tests for search error types

#[cfg(test)]
mod tests {
    use crate::db::error::DbError;
    use crate::search::error::SearchError;
    use std::error::Error;

    #[test]
    fn test_interrupted_error() {
        let error = SearchError::InterruptedError;
        assert_eq!(error.to_string(), "Interactive selection was interrupted");
    }

    #[test]
    fn test_build_error() {
        let error = SearchError::BuildError("Invalid options".to_string());
        assert_eq!(
            error.to_string(),
            "Failed to build UI options: Invalid options"
        );
    }

    #[test]
    fn test_database_error_from_db_error() {
        let db_error = DbError::FileNotFound("test.txt".to_string());
        let search_error: SearchError = db_error.into();

        assert!(search_error.to_string().contains("Database error"));
    }

    #[test]
    fn test_error_display() {
        let error = SearchError::BuildError("test error".to_string());
        let display = format!("{error}");
        assert!(display.contains("Failed to build UI options"));
        assert!(display.contains("test error"));
    }

    #[test]
    fn test_error_debug() {
        let error = SearchError::InterruptedError;
        let debug = format!("{error:?}");
        assert!(debug.contains("InterruptedError"));
    }

    #[test]
    fn test_build_error_creation() {
        let msg = "Height must be positive";
        let error = SearchError::BuildError(msg.to_string());

        match error {
            SearchError::BuildError(s) => assert_eq!(s, msg),
            _ => panic!("Expected BuildError variant"),
        }
    }

    #[test]
    fn test_interrupted_error_creation() {
        let error = SearchError::InterruptedError;

        match error {
            SearchError::InterruptedError => {
                // Success
            }
            _ => panic!("Expected InterruptedError variant"),
        }
    }

    #[test]
    fn test_error_source_chain() {
        let db_error = DbError::SerializeError("UTF-8 error".to_string());
        let search_error = SearchError::DatabaseError(db_error);

        // Should have a source (the wrapped DbError)
        assert!(search_error.source().is_some());
    }

    #[test]
    fn test_error_source_none() {
        let error = SearchError::InterruptedError;
        // InterruptedError has no source
        assert!(error.source().is_none());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SearchError>();
    }

    #[test]
    fn test_error_pattern_matching() {
        let errors = vec![
            SearchError::InterruptedError,
            SearchError::BuildError("error1".to_string()),
            SearchError::DatabaseError(DbError::FileNotFound("file.txt".to_string())),
        ];

        for error in errors {
            match error {
                SearchError::BuildError(msg) => {
                    assert_eq!(msg, "error1");
                }
                SearchError::InterruptedError | SearchError::DatabaseError(_) | SearchError::UiError(_) => {
                    // Expected
                }
            }
        }
    }

    #[test]
    fn test_from_db_error_file_not_found() {
        let db_error = DbError::FileNotFound("missing.txt".to_string());
        let search_error = SearchError::from(db_error);

        match search_error {
            SearchError::DatabaseError(e) => {
                assert!(e.to_string().contains("File not found"));
                assert!(e.to_string().contains("missing.txt"));
            }
            _ => panic!("Expected DatabaseError variant"),
        }
    }

    #[test]
    fn test_from_db_error_serialize() {
        let db_error = DbError::SerializeError("Invalid data".to_string());
        let search_error = SearchError::from(db_error);

        match search_error {
            SearchError::DatabaseError(e) => {
                assert!(e.to_string().contains("serialization"));
            }
            _ => panic!("Expected DatabaseError variant"),
        }
    }

    #[test]
    fn test_error_chaining() {
        let db_err = DbError::FileNotFound("file.txt".to_string());
        let search_err = SearchError::DatabaseError(db_err);

        let error_msg = search_err.to_string();
        assert!(error_msg.contains("Database error"));
        assert!(error_msg.contains("File not found"));
    }

    #[test]
    fn test_multiple_error_types() {
        let errors: Vec<SearchError> = vec![
            SearchError::InterruptedError,
            SearchError::BuildError("UI error".to_string()),
            SearchError::DatabaseError(DbError::FileNotFound("test.txt".to_string())),
        ];

        assert_eq!(errors.len(), 3);

        let messages: Vec<String> = errors
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert!(messages[0].contains("interrupted"));
        assert!(messages[1].contains("UI options"));
        assert!(messages[2].contains("Database error"));
    }

    #[test]
    fn test_ui_error_conversion() {
        let ui_error = crate::ui::UiError::InterruptedError;
        let search_error = SearchError::from(ui_error);

        match search_error {
            SearchError::UiError(_) => {},
            _ => panic!("Expected UiError variant"),
        }
    }
}
