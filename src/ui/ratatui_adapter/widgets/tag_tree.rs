//! Tag hierarchy tree widget for displaying tags with parent-child relationships

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, StatefulWidget, Widget},
};
use std::collections::{HashMap, HashSet};

/// A node in the tag tree (can be tag or inferred parent)
#[derive(Debug, Clone)]
pub struct TagTreeNode {
    /// Tag name (just this level, not full path)
    pub name: String,
    /// Full tag path (e.g., "lang:rust:async")
    pub full_path: String,
    /// Number of files with this exact tag
    pub file_count: usize,
    /// Children nodes (sorted alphabetically)
    pub children: Vec<TagTreeNode>,
    /// Whether this is an actual tag or inferred parent node
    pub is_actual_tag: bool,
    /// Whether this node is currently expanded
    pub is_expanded: bool,
    /// Depth level (0 = root)
    pub depth: usize,
}

/// State for the tag tree widget
#[derive(Debug, Clone)]
pub struct TagTreeState {
    /// Root nodes of the tree
    pub roots: Vec<TagTreeNode>,
    /// Currently selected node index (in flattened visible list)
    pub selected: usize,
    /// Scroll offset
    pub scroll_offset: usize,
    /// Cache of flattened visible nodes for navigation
    visible_nodes: Vec<TagTreeNodeRef>,
    /// Set of selected tag paths (for multi-select)
    pub selected_tags: HashSet<String>,
}

/// Reference to a node in the tree (for flattened view)
#[derive(Debug, Clone)]
struct TagTreeNodeRef {
    /// Full path to the node
    full_path: String,
    /// Display name
    name: String,
    /// File count
    file_count: usize,
    /// Depth level
    depth: usize,
    /// Whether it's an actual tag
    is_actual_tag: bool,
}

impl TagTreeNode {
    /// Create a new tag tree node
    #[must_use]
    pub fn new(
        name: String,
        full_path: String,
        file_count: usize,
        is_actual_tag: bool,
        depth: usize,
    ) -> Self {
        Self {
            name,
            full_path,
            file_count,
            children: Vec::new(),
            is_actual_tag,
            is_expanded: true, // Expanded by default
            depth,
        }
    }

    /// Add a child node
    pub fn add_child(&mut self, child: TagTreeNode) {
        self.children.push(child);
        // Keep children sorted
        self.children
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    /// Toggle expansion state
    pub fn toggle_expand(&mut self) {
        self.is_expanded = !self.is_expanded;
    }

    /// Recursively collect all visible nodes (for flattened navigation)
    fn collect_visible(&self, output: &mut Vec<TagTreeNodeRef>) {
        output.push(TagTreeNodeRef {
            full_path: self.full_path.clone(),
            name: self.name.clone(),
            file_count: self.file_count,
            depth: self.depth,
            is_actual_tag: self.is_actual_tag,
        });

        if self.is_expanded {
            for child in &self.children {
                child.collect_visible(output);
            }
        }
    }
}

impl TagTreeState {
    /// Create empty tag tree state
    #[must_use]
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            visible_nodes: Vec::new(),
            selected_tags: HashSet::new(),
        }
    }

    /// Build tree from flat tag list with file counts
    ///
    /// Takes tags like ["rust", "lang:rust", "lang:python"] and builds a tree:
    /// - lang (inferred parent)
    ///   ├── rust
    ///   └── python
    /// - rust (standalone tag)
    pub fn build_from_tags(&mut self, tags: Vec<(String, usize)>) {
        let mut hierarchy_map: HashMap<String, Vec<(String, usize, bool)>> = HashMap::new();
        let mut actual_tags: HashSet<String> = HashSet::new();

        // First pass: identify all actual tags and their hierarchy
        for (tag, count) in &tags {
            actual_tags.insert(tag.clone());

            if tag.contains(':') {
                // Hierarchical tag - split into levels
                let parts: Vec<&str> = tag.split(':').collect();

                // Add each level to hierarchy
                for i in 0..parts.len() {
                    let partial_path = parts[..=i].join(":");
                    let is_actual = actual_tags.contains(&partial_path);
                    let level_count = if i == parts.len() - 1 { *count } else { 0 };

                    hierarchy_map
                        .entry(partial_path.clone())
                        .or_default()
                        .push((tag.clone(), level_count, is_actual));

                    // Also track parent relationships
                    if i > 0 {
                        let parent_path = parts[..i].join(":");
                        hierarchy_map.entry(parent_path).or_default();
                    }
                }
            } else {
                // Root-level tag
                hierarchy_map
                    .entry(tag.clone())
                    .or_default()
                    .push((tag.clone(), *count, true));
            }
        }

        // Build tree recursively
        self.roots = Self::build_level(&hierarchy_map, &actual_tags, "", 0);
        self.rebuild_visible_cache();
    }

    /// Build nodes at a specific level
    fn build_level(
        hierarchy_map: &HashMap<String, Vec<(String, usize, bool)>>,
        actual_tags: &HashSet<String>,
        parent_path: &str,
        depth: usize,
    ) -> Vec<TagTreeNode> {
        let mut nodes = Vec::new();
        let mut seen = HashSet::new();

        // Collect all child prefixes at this level
        let prefix = if parent_path.is_empty() {
            String::new()
        } else {
            format!("{parent_path}:")
        };

        for full_tag in actual_tags.iter() {
            if parent_path.is_empty() {
                // Root level - get first component
                let first_part = full_tag.split(':').next().unwrap_or(full_tag);
                if seen.insert(first_part.to_string()) {
                    let full_path = first_part.to_string();
                    let is_actual = actual_tags.contains(&full_path);
                    let file_count = if is_actual {
                        hierarchy_map
                            .get(&full_path)
                            .and_then(|v| v.first())
                            .map_or(0, |(_, c, _)| *c)
                    } else {
                        0
                    };

                    let mut node = TagTreeNode::new(
                        first_part.to_string(),
                        full_path.clone(),
                        file_count,
                        is_actual,
                        depth,
                    );

                    // Recursively build children
                    node.children =
                        Self::build_level(hierarchy_map, actual_tags, &full_path, depth + 1);

                    nodes.push(node);
                }
            } else if full_tag.starts_with(&prefix) {
                // This tag is a child of parent_path
                let remainder = &full_tag[prefix.len()..];
                let next_part = remainder.split(':').next().unwrap_or(remainder);

                if seen.insert(next_part.to_string()) {
                    let full_path = format!("{parent_path}:{next_part}");
                    let is_actual = actual_tags.contains(&full_path);
                    let file_count = if is_actual {
                        hierarchy_map
                            .get(&full_path)
                            .and_then(|v| v.first())
                            .map_or(0, |(_, c, _)| *c)
                    } else {
                        0
                    };

                    let mut node = TagTreeNode::new(
                        next_part.to_string(),
                        full_path.clone(),
                        file_count,
                        is_actual,
                        depth,
                    );

                    // Recursively build children
                    node.children =
                        Self::build_level(hierarchy_map, actual_tags, &full_path, depth + 1);

                    nodes.push(node);
                }
            }
        }

        // Sort nodes alphabetically
        nodes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        nodes
    }

    /// Rebuild the visible nodes cache
    fn rebuild_visible_cache(&mut self) {
        self.visible_nodes.clear();
        for root in &self.roots {
            root.collect_visible(&mut self.visible_nodes);
        }
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.visible_nodes.len() {
            self.selected += 1;
        }
    }

    /// Toggle expansion of currently selected node
    pub fn toggle_selected(&mut self) {
        if let Some(node_ref) = self.visible_nodes.get(self.selected) {
            let path = node_ref.full_path.clone();
            self.toggle_node_by_path(&path);
            self.rebuild_visible_cache();
        }
    }

    /// Toggle a node by its full path
    fn toggle_node_by_path(&mut self, path: &str) -> bool {
        for root in &mut self.roots {
            if Self::toggle_node_recursive(root, path) {
                return true;
            }
        }
        false
    }

    /// Recursively find and toggle a node
    fn toggle_node_recursive(node: &mut TagTreeNode, path: &str) -> bool {
        if node.full_path == path {
            node.toggle_expand();
            return true;
        }

        for child in &mut node.children {
            if Self::toggle_node_recursive(child, path) {
                return true;
            }
        }

        false
    }

    /// Toggle selection of current tag
    pub fn toggle_tag_selection(&mut self) {
        if let Some(node_ref) = self.visible_nodes.get(self.selected) {
            if node_ref.is_actual_tag {
                let path = node_ref.full_path.clone();
                if self.selected_tags.contains(&path) {
                    self.selected_tags.remove(&path);
                } else {
                    self.selected_tags.insert(path);
                }
            }
        }
    }

    /// Get currently selected tag path (if any)
    #[must_use]
    pub fn current_tag(&self) -> Option<String> {
        self.visible_nodes
            .get(self.selected)
            .map(|n| n.full_path.clone())
    }

    /// Get all selected tag paths
    #[must_use]
    pub fn selected_tag_paths(&self) -> Vec<String> {
        self.selected_tags.iter().cloned().collect()
    }

    /// Get count of visible nodes
    #[must_use]
    pub fn visible_count(&self) -> usize {
        self.visible_nodes.len()
    }
}

impl Default for TagTreeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tag tree widget for rendering
pub struct TagTree<'a> {
    block: Option<Block<'a>>,
    highlight_style: Style,
    selected_style: Style,
    normal_style: Style,
    inferred_style: Style,
}

impl<'a> TagTree<'a> {
    /// Create new tag tree widget
    #[must_use]
    pub fn new() -> Self {
        Self {
            block: None,
            highlight_style: Style::default().add_modifier(Modifier::REVERSED),
            selected_style: Style::default().add_modifier(Modifier::BOLD),
            normal_style: Style::default(),
            inferred_style: Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// Set border block
    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Set highlight style
    #[must_use]
    pub const fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }
}

impl<'a> Default for TagTree<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for TagTree<'_> {
    type State = TagTreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let area = self.block.as_ref().map_or(area, |b| {
            let inner = b.inner(area);
            b.clone().render(area, buf);
            inner
        });

        if area.height == 0 {
            return;
        }

        let visible_height = area.height as usize;

        // Adjust scroll offset to keep selected item visible
        if state.selected < state.scroll_offset {
            state.scroll_offset = state.selected;
        } else if state.selected >= state.scroll_offset + visible_height {
            state.scroll_offset = state.selected.saturating_sub(visible_height - 1);
        }

        // Render visible nodes
        let start = state.scroll_offset;
        let end = (start + visible_height).min(state.visible_nodes.len());

        for (i, node_ref) in state.visible_nodes[start..end].iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let is_selected = start + i == state.selected;
            let is_tag_selected = state.selected_tags.contains(&node_ref.full_path);

            // Build the line with tree characters
            let mut spans = Vec::new();

            // Indentation
            let indent = "  ".repeat(node_ref.depth);
            spans.push(Span::raw(indent));

            // Tree character (├── or └──)
            let tree_char = if node_ref.depth > 0 { "├── " } else { "" };
            spans.push(Span::raw(tree_char));

            // Selection checkmark for actual tags (green ✓)
            if node_ref.is_actual_tag {
                if is_tag_selected {
                    spans.push(Span::styled("✓ ", Style::default().fg(Color::Green)));
                } else {
                    spans.push(Span::raw("  "));
                }
            }

            // Tag name
            let name_style = if is_selected {
                self.highlight_style
            } else if is_tag_selected {
                self.selected_style
            } else if node_ref.is_actual_tag {
                self.normal_style
            } else {
                self.inferred_style
            };

            let display_text = if node_ref.is_actual_tag {
                format!("{} ({})", node_ref.name, node_ref.file_count)
            } else {
                format!("{} (parent)", node_ref.name)
            };

            spans.push(Span::styled(display_text, name_style));

            let line = Line::from(spans);
            buf.set_line(area.x, y, &line, area.width);
        }
    }
}

/// Widget factory for creating a tag tree with a border
#[must_use]
pub fn tag_tree_with_border<'a>(title: &'a str) -> TagTree<'a> {
    TagTree::new().block(Block::default().borders(Borders::ALL).title(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_tree_node_creation() {
        let node = TagTreeNode::new("rust".to_string(), "rust".to_string(), 42, true, 0);

        assert_eq!(node.name, "rust");
        assert_eq!(node.full_path, "rust");
        assert_eq!(node.file_count, 42);
        assert!(node.is_actual_tag);
        assert_eq!(node.depth, 0);
        assert!(node.is_expanded);
    }

    #[test]
    fn test_tag_tree_state_build_flat() {
        let mut state = TagTreeState::new();

        let tags = vec![
            ("rust".to_string(), 10),
            ("python".to_string(), 20),
            ("javascript".to_string(), 15),
        ];

        state.build_from_tags(tags);

        assert_eq!(state.roots.len(), 3);
        assert_eq!(state.visible_count(), 3);
    }

    #[test]
    fn test_tag_tree_state_build_hierarchical() {
        let mut state = TagTreeState::new();

        let tags = vec![
            ("lang:rust".to_string(), 10),
            ("lang:python".to_string(), 20),
            ("lang:rust:async".to_string(), 5),
        ];

        state.build_from_tags(tags);

        // Should have one root: "lang" (inferred parent)
        assert_eq!(state.roots.len(), 1);
        assert_eq!(state.roots[0].name, "lang");
        assert!(!state.roots[0].is_actual_tag);

        // "lang" should have 2 children: rust, python
        assert_eq!(state.roots[0].children.len(), 2);

        // "rust" should have 1 child: async
        let rust_node = state.roots[0]
            .children
            .iter()
            .find(|n| n.name == "rust")
            .unwrap();
        assert_eq!(rust_node.children.len(), 1);
        assert_eq!(rust_node.children[0].name, "async");
    }

    #[test]
    fn test_tag_tree_navigation() {
        let mut state = TagTreeState::new();

        let tags = vec![
            ("tag1".to_string(), 10),
            ("tag2".to_string(), 20),
            ("tag3".to_string(), 30),
        ];

        state.build_from_tags(tags);

        assert_eq!(state.selected, 0);

        state.move_down();
        assert_eq!(state.selected, 1);

        state.move_down();
        assert_eq!(state.selected, 2);

        state.move_down(); // Should not go beyond
        assert_eq!(state.selected, 2);

        state.move_up();
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_tag_tree_selection() {
        let mut state = TagTreeState::new();

        let tags = vec![("tag1".to_string(), 10), ("tag2".to_string(), 20)];

        state.build_from_tags(tags);

        assert!(state.selected_tags.is_empty());

        state.toggle_tag_selection();
        assert_eq!(state.selected_tags.len(), 1);
        assert!(state.selected_tags.contains("tag1"));

        state.move_down();
        state.toggle_tag_selection();
        assert_eq!(state.selected_tags.len(), 2);

        state.toggle_tag_selection();
        assert_eq!(state.selected_tags.len(), 1);
    }
}
