// split_tree.rs - Split layout tree for multi-pane display
use serde::{Deserialize, Serialize};

/// A node in the split layout tree - either a leaf pane or a split container
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SplitNode {
    Pane { target: String },
    Vertical {
        ratio: f32,
        left: Box<SplitNode>,
        right: Box<SplitNode>,
    },
    Horizontal {
        ratio: f32,
        top: Box<SplitNode>,
        bottom: Box<SplitNode>,
    },
}

impl SplitNode {
    /// Minimum ratio to prevent panes from becoming too small
    pub const MIN_RATIO: f32 = 0.1;
    /// Maximum ratio to prevent panes from becoming too small
    pub const MAX_RATIO: f32 = 0.9;

    /// Create a single-pane node
    pub fn pane(target: impl Into<String>) -> Self {
        SplitNode::Pane {
            target: target.into(),
        }
    }

    /// Flatten the tree to a list of (pane_target, ratio_hint) in left-to-right, top-to-bottom order.
    /// ratio_hint is used for rendering; for leaves it's 1.0 (full size in its container).
    pub fn flatten(&self) -> Vec<(String, f32)> {
        let mut out = Vec::new();
        self.flatten_impl(1.0, &mut out);
        out
    }

    fn flatten_impl(&self, parent_ratio: f32, out: &mut Vec<(String, f32)>) {
        match self {
            SplitNode::Pane { target } => {
                out.push((target.clone(), parent_ratio));
            }
            SplitNode::Vertical { ratio, left, right } => {
                let r = ratio.clamp(Self::MIN_RATIO, Self::MAX_RATIO);
                left.flatten_impl(parent_ratio * r, out);
                right.flatten_impl(parent_ratio * (1.0 - r), out);
            }
            SplitNode::Horizontal { ratio, top, bottom } => {
                let r = ratio.clamp(Self::MIN_RATIO, Self::MAX_RATIO);
                top.flatten_impl(parent_ratio * r, out);
                bottom.flatten_impl(parent_ratio * (1.0 - r), out);
            }
        }
    }

    /// Get pane target at focus index (0-based)
    pub fn focus_index_to_pane_target(&self, index: usize) -> Option<String> {
        let flat = self.flatten();
        flat.get(index).map(|(t, _)| t.clone())
    }

    /// Number of panes in this tree
    pub fn pane_count(&self) -> usize {
        self.flatten().len()
    }

    /// Split the focused pane (at index) into a new split. Creates a new pane via tmux and updates the tree.
    /// Returns the new SplitNode and the new pane target (for caller to register).
    /// The caller must call tmux split_pane_vertical/horizontal before calling this.
    pub fn split_at_focused(
        &self,
        focused_index: usize,
        vertical: bool,
        new_pane_target: String,
    ) -> Option<SplitNode> {
        self.split_at_index_impl(0, focused_index, vertical, &new_pane_target)
            .map(|(node, _)| node)
    }

    fn split_at_index_impl(
        &self,
        current_index: usize,
        target_index: usize,
        vertical: bool,
        new_pane_target: &str,
    ) -> Option<(SplitNode, usize)> {
        match self {
            SplitNode::Pane { target } => {
                if current_index == target_index {
                    let new_node = if vertical {
                        SplitNode::Vertical {
                            ratio: 0.5,
                            left: Box::new(SplitNode::Pane {
                                target: target.clone(),
                            }),
                            right: Box::new(SplitNode::Pane {
                                target: new_pane_target.to_string(),
                            }),
                        }
                    } else {
                        SplitNode::Horizontal {
                            ratio: 0.5,
                            top: Box::new(SplitNode::Pane {
                                target: target.clone(),
                            }),
                            bottom: Box::new(SplitNode::Pane {
                                target: new_pane_target.to_string(),
                            }),
                        }
                    };
                    Some((new_node, current_index + 1))
                } else {
                    None
                }
            }
            SplitNode::Vertical { ratio, left, right } => {
                let left_count = left.pane_count();
                if target_index < current_index + left_count {
                    let (new_left, idx) =
                        left.split_at_index_impl(current_index, target_index, vertical, new_pane_target)?;
                    Some((
                        SplitNode::Vertical {
                            ratio: *ratio,
                            left: Box::new(new_left),
                            right: right.clone(),
                        },
                        idx,
                    ))
                } else {
                    let (new_right, idx) = right.split_at_index_impl(
                        current_index + left_count,
                        target_index,
                        vertical,
                        new_pane_target,
                    )?;
                    Some((
                        SplitNode::Vertical {
                            ratio: *ratio,
                            left: left.clone(),
                            right: Box::new(new_right),
                        },
                        idx,
                    ))
                }
            }
            SplitNode::Horizontal { ratio, top, bottom } => {
                let top_count = top.pane_count();
                if target_index < current_index + top_count {
                    let (new_top, idx) =
                        top.split_at_index_impl(current_index, target_index, vertical, new_pane_target)?;
                    Some((
                        SplitNode::Horizontal {
                            ratio: *ratio,
                            top: Box::new(new_top),
                            bottom: bottom.clone(),
                        },
                        idx,
                    ))
                } else {
                    let (new_bottom, idx) = bottom.split_at_index_impl(
                        current_index + top_count,
                        target_index,
                        vertical,
                        new_pane_target,
                    )?;
                    Some((
                        SplitNode::Horizontal {
                            ratio: *ratio,
                            top: top.clone(),
                            bottom: Box::new(new_bottom),
                        },
                        idx,
                    ))
                }
            }
        }
    }

    /// Update ratio at a specific split. The path is a sequence of 0/1 for left/right or top/bottom.
    /// Returns true if the ratio was updated.
    pub fn update_ratio(&mut self, path: &[bool], new_ratio: f32) -> bool {
        let r = new_ratio.clamp(Self::MIN_RATIO, Self::MAX_RATIO);
        if path.is_empty() {
            match self {
                SplitNode::Vertical { ratio, .. } | SplitNode::Horizontal { ratio, .. } => {
                    *ratio = r;
                    true
                }
                SplitNode::Pane { .. } => false,
            }
        } else {
            let (head, tail) = (path[0], &path[1..]);
            match self {
                SplitNode::Vertical { left, right, .. } => {
                    if head {
                        right.update_ratio(tail, r)
                    } else {
                        left.update_ratio(tail, r)
                    }
                }
                SplitNode::Horizontal { top, bottom, .. } => {
                    if head {
                        bottom.update_ratio(tail, r)
                    } else {
                        top.update_ratio(tail, r)
                    }
                }
                SplitNode::Pane { .. } => false,
            }
        }
    }

    /// Get the path to the split at the given divider index (0 = first divider, etc).
    /// Dividers are ordered left-to-right, top-to-bottom.
    pub fn divider_path(&self, divider_index: usize) -> Option<Vec<bool>> {
        let mut path = Vec::new();
        if self.divider_path_impl(divider_index, 0, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn divider_path_impl(&self, target: usize, current: usize, path: &mut Vec<bool>) -> bool {
        match self {
            SplitNode::Pane { .. } => false,
            SplitNode::Vertical { left, right, .. } => {
                let left_dividers = left.divider_count();
                if target == current {
                    true
                } else if target < current + 1 + left_dividers {
                    path.push(false);
                    left.divider_path_impl(target, current + 1, path)
                } else {
                    path.push(true);
                    right.divider_path_impl(target, current + 1 + left_dividers, path)
                }
            }
            SplitNode::Horizontal { top, bottom, .. } => {
                let top_dividers = top.divider_count();
                if target == current {
                    true
                } else if target < current + 1 + top_dividers {
                    path.push(false);
                    top.divider_path_impl(target, current + 1, path)
                } else {
                    path.push(true);
                    bottom.divider_path_impl(target, current + 1 + top_dividers, path)
                }
            }
        }
    }

    /// Number of dividers (splits) in the tree
    pub fn divider_count(&self) -> usize {
        match self {
            SplitNode::Pane { .. } => 0,
            SplitNode::Vertical { left, right, .. } => {
                1 + left.divider_count() + right.divider_count()
            }
            SplitNode::Horizontal { top, bottom, .. } => {
                1 + top.divider_count() + bottom.divider_count()
            }
        }
    }

    /// Remove pane at index and collapse the tree. Returns None if only one pane.
    /// None from inner means "this pane was removed, use sibling".
    pub fn remove_pane_at_index(&self, index: usize) -> Option<SplitNode> {
        if self.pane_count() <= 1 {
            return None;
        }
        self.remove_pane_impl(0, index)
    }

    fn remove_pane_impl(&self, current: usize, target: usize) -> Option<SplitNode> {
        match self {
            SplitNode::Pane { .. } => {
                if current == target {
                    None // Signal to parent: use sibling
                } else {
                    None // Should not reach: target out of our range
                }
            }
            SplitNode::Vertical { ratio, left, right } => {
                let left_count = left.pane_count();
                if target < current + left_count {
                    match left.remove_pane_impl(current, target) {
                        Some(new_left) => Some(SplitNode::Vertical {
                            ratio: *ratio,
                            left: Box::new(new_left),
                            right: right.clone(),
                        }),
                        None => Some((**right).clone()),
                    }
                } else {
                    match right.remove_pane_impl(current + left_count, target) {
                        Some(new_right) => Some(SplitNode::Vertical {
                            ratio: *ratio,
                            left: left.clone(),
                            right: Box::new(new_right),
                        }),
                        None => Some((**left).clone()),
                    }
                }
            }
            SplitNode::Horizontal { ratio, top, bottom } => {
                let top_count = top.pane_count();
                if target < current + top_count {
                    match top.remove_pane_impl(current, target) {
                        Some(new_top) => Some(SplitNode::Horizontal {
                            ratio: *ratio,
                            top: Box::new(new_top),
                            bottom: bottom.clone(),
                        }),
                        None => Some((**bottom).clone()),
                    }
                } else {
                    match bottom.remove_pane_impl(current + top_count, target) {
                        Some(new_bottom) => Some(SplitNode::Horizontal {
                            ratio: *ratio,
                            top: top.clone(),
                            bottom: Box::new(new_bottom),
                        }),
                        None => Some((**top).clone()),
                    }
                }
            }
        }
    }

    /// Get ratio at a path (for reading)
    pub fn ratio_at(&self, path: &[bool]) -> Option<f32> {
        if path.is_empty() {
            match self {
                SplitNode::Vertical { ratio, .. } | SplitNode::Horizontal { ratio, .. } => {
                    Some(*ratio)
                }
                SplitNode::Pane { .. } => None,
            }
        } else {
            let (head, tail) = (path[0], &path[1..]);
            match self {
                SplitNode::Vertical { left, right, .. } => {
                    if head {
                        right.ratio_at(tail)
                    } else {
                        left.ratio_at(tail)
                    }
                }
                SplitNode::Horizontal { top, bottom, .. } => {
                    if head {
                        bottom.ratio_at(tail)
                    } else {
                        top.ratio_at(tail)
                    }
                }
                SplitNode::Pane { .. } => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatten_single() {
        let node = SplitNode::pane("sess:win.0");
        let flat = node.flatten();
        assert_eq!(flat, vec![("sess:win.0".to_string(), 1.0)]);
    }

    #[test]
    fn test_flatten_vertical() {
        let node = SplitNode::Vertical {
            ratio: 0.5,
            left: Box::new(SplitNode::pane("a")),
            right: Box::new(SplitNode::pane("b")),
        };
        let flat = node.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].0, "a");
        assert_eq!(flat[1].0, "b");
    }

    #[test]
    fn test_focus_index_to_pane_target() {
        let node = SplitNode::Vertical {
            ratio: 0.5,
            left: Box::new(SplitNode::pane("left")),
            right: Box::new(SplitNode::pane("right")),
        };
        assert_eq!(node.focus_index_to_pane_target(0), Some("left".into()));
        assert_eq!(node.focus_index_to_pane_target(1), Some("right".into()));
        assert_eq!(node.focus_index_to_pane_target(2), None);
    }

    #[test]
    fn test_split_at_focused() {
        let node = SplitNode::pane("sess:win.0");
        let new_node = node.split_at_focused(0, true, "sess:win.1".to_string());
        assert!(new_node.is_some());
        let new_node = new_node.unwrap();
        let flat = new_node.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].0, "sess:win.0");
        assert_eq!(flat[1].0, "sess:win.1");
    }

    #[test]
    fn test_remove_pane_at_index() {
        let node = SplitNode::Vertical {
            ratio: 0.5,
            left: Box::new(SplitNode::pane("a")),
            right: Box::new(SplitNode::pane("b")),
        };
        let removed = node.remove_pane_at_index(0);
        assert!(removed.is_some());
        let new_node = removed.unwrap();
        assert_eq!(new_node.pane_count(), 1);
        assert_eq!(new_node.flatten()[0].0, "b");

        let removed1 = node.remove_pane_at_index(1);
        assert!(removed1.is_some());
        let new_node1 = removed1.unwrap();
        assert_eq!(new_node1.pane_count(), 1);
        assert_eq!(new_node1.flatten()[0].0, "a");

        let single = SplitNode::pane("x");
        assert!(single.remove_pane_at_index(0).is_none());
    }

    #[test]
    fn test_divider_count() {
        let node = SplitNode::pane("a");
        assert_eq!(node.divider_count(), 0);

        let node = SplitNode::Vertical {
            ratio: 0.5,
            left: Box::new(SplitNode::pane("a")),
            right: Box::new(SplitNode::pane("b")),
        };
        assert_eq!(node.divider_count(), 1);
    }
}
