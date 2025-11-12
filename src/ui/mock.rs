//! Mock fuzzy finder for testing

use super::error::Result;
use super::traits::{FinderConfig, FuzzyFinder};
use super::types::FinderResult;

/// Mock fuzzy finder that returns predetermined selections
///
/// Useful for testing without requiring user interaction
#[derive(Debug, Clone)]
pub struct MockFinder {
    /// Predetermined items to return as selected
    pub predetermined_selection: Vec<String>,
    /// Whether to simulate user abort
    pub should_abort: bool,
}

impl MockFinder {
    /// Create a new mock finder with predetermined selections
    #[must_use]
    pub fn new(selections: Vec<String>) -> Self {
        Self {
            predetermined_selection: selections,
            should_abort: false,
        }
    }

    /// Create a mock finder that simulates user abort
    #[must_use]
    pub fn aborted() -> Self {
        Self {
            predetermined_selection: Vec::new(),
            should_abort: true,
        }
    }
}

impl Default for MockFinder {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl FuzzyFinder for MockFinder {
    fn run(&self, _config: FinderConfig) -> Result<FinderResult> {
        if self.should_abort {
            Ok(FinderResult::aborted())
        } else {
            Ok(FinderResult::selected(
                self.predetermined_selection.clone(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::DisplayItem;

    #[test]
    fn test_mock_finder_with_selections() {
        let finder = MockFinder::new(vec!["file1.txt".to_string(), "file2.txt".to_string()]);
        let config = FinderConfig::new(vec![], "test: ".to_string());
        let result = finder.run(config).unwrap();

        assert!(!result.aborted);
        assert_eq!(result.selected.len(), 2);
        assert_eq!(result.selected[0], "file1.txt");
        assert_eq!(result.selected[1], "file2.txt");
    }

    #[test]
    fn test_mock_finder_aborted() {
        let finder = MockFinder::aborted();
        let config = FinderConfig::new(vec![], "test: ".to_string());
        let result = finder.run(config).unwrap();

        assert!(result.aborted);
        assert!(result.selected.is_empty());
    }

    #[test]
    fn test_mock_finder_empty_selection() {
        let finder = MockFinder::default();
        let config = FinderConfig::new(
            vec![DisplayItem::new(
                "file.txt".to_string(),
                "file.txt".to_string(),
                "file.txt".to_string(),
            )],
            "test: ".to_string(),
        );
        let result = finder.run(config).unwrap();

        assert!(!result.aborted);
        assert!(result.selected.is_empty());
    }
}
