use uuid::Uuid;

pub(super) const MAX_TERMINAL_PANES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

impl PaneDirection {
    pub(super) fn from_command(command: &str) -> Option<(Self, PaneCommand)> {
        match command {
            "focus-left" => Some((Self::Left, PaneCommand::Focus)),
            "focus-right" => Some((Self::Right, PaneCommand::Focus)),
            "focus-up" => Some((Self::Up, PaneCommand::Focus)),
            "focus-down" => Some((Self::Down, PaneCommand::Focus)),
            "split-left" => Some((Self::Left, PaneCommand::Split)),
            "split-right" => Some((Self::Right, PaneCommand::Split)),
            "split-up" => Some((Self::Up, PaneCommand::Split)),
            "split-down" => Some((Self::Down, PaneCommand::Split)),
            _ => None,
        }
    }

    const fn is_before(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }

    const fn split_axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Columns,
            Self::Up | Self::Down => SplitAxis::Rows,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaneCommand {
    Focus,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PanePlacement {
    pub(super) tab_id: Uuid,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaneRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl PaneRect {
    const fn right(self) -> f32 {
        self.x + self.width
    }

    const fn bottom(self) -> f32 {
        self.y + self.height
    }

    const fn center_x(self) -> f32 {
        self.x + self.width / 2.0
    }

    const fn center_y(self) -> f32 {
        self.y + self.height / 2.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitAxis {
    Columns,
    Rows,
}

#[derive(Clone, Debug)]
enum PaneNode {
    Leaf(Uuid),
    Split {
        axis: SplitAxis,
        first: Box<Self>,
        second: Box<Self>,
    },
}

/// A bounded, per-window terminal layout. It carries only terminal Tab UUIDs
/// and intentionally has no persistence, UI handle, worker, or credential state.
#[derive(Clone, Debug)]
pub(super) struct PaneTree {
    root: PaneNode,
    focused_tab_id: Uuid,
}

impl PaneTree {
    pub(super) fn new(tab_id: Uuid) -> Self {
        Self {
            root: PaneNode::Leaf(tab_id),
            focused_tab_id: tab_id,
        }
    }

    pub(super) fn contains(&self, tab_id: Uuid) -> bool {
        self.tab_ids().contains(&tab_id)
    }

    #[cfg(test)]
    pub(super) const fn focused_tab_id(&self) -> Uuid {
        self.focused_tab_id
    }

    pub(super) fn set_focused(&mut self, tab_id: Uuid) -> bool {
        if !self.contains(tab_id) {
            return false;
        }
        self.focused_tab_id = tab_id;
        true
    }

    pub(super) fn pane_count(&self) -> usize {
        self.tab_ids().len()
    }

    pub(super) fn tab_ids(&self) -> Vec<Uuid> {
        let mut ids = Vec::new();
        collect_tab_ids(&self.root, &mut ids);
        ids
    }

    pub(super) fn root_tab_id(&self) -> Uuid {
        first_tab_id(&self.root)
    }

    pub(super) fn placements(&self) -> Vec<PanePlacement> {
        let mut placements = Vec::with_capacity(self.pane_count());
        collect_placements(
            &self.root,
            PaneRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            self.focused_tab_id,
            &mut placements,
        );
        placements
    }

    pub(super) fn split_focused(&mut self, direction: PaneDirection, new_tab_id: Uuid) -> bool {
        if self.pane_count() >= MAX_TERMINAL_PANES || self.contains(new_tab_id) {
            return false;
        }
        let source = self.focused_tab_id;
        if !replace_leaf_with_split(&mut self.root, source, direction, new_tab_id) {
            return false;
        }
        self.focused_tab_id = new_tab_id;
        true
    }

    pub(super) fn focus_direction(&mut self, direction: PaneDirection) -> Option<Uuid> {
        let placements = self.placements();
        let current = placements
            .iter()
            .find(|placement| placement.tab_id == self.focused_tab_id)?;
        let current_rect = PaneRect {
            x: current.x,
            y: current.y,
            width: current.width,
            height: current.height,
        };
        let next = placements
            .into_iter()
            .filter(|placement| placement.tab_id != self.focused_tab_id)
            .filter_map(|placement| {
                let candidate = PaneRect {
                    x: placement.x,
                    y: placement.y,
                    width: placement.width,
                    height: placement.height,
                };
                directional_distance(current_rect, candidate, direction)
                    .map(|distance| (placement, distance))
            })
            .min_by(|(_, left), (_, right)| left.total_cmp(right))?
            .0;
        self.focused_tab_id = next.tab_id;
        Some(next.tab_id)
    }

    pub(super) fn remove(&mut self, tab_id: Uuid) -> Option<Uuid> {
        if !self.contains(tab_id) {
            return Some(self.focused_tab_id);
        }
        let root = remove_leaf(self.root.clone(), tab_id)?;
        self.root = root;
        if self.focused_tab_id == tab_id {
            self.focused_tab_id = self.root_tab_id();
        }
        Some(self.focused_tab_id)
    }
}

fn collect_tab_ids(node: &PaneNode, ids: &mut Vec<Uuid>) {
    match node {
        PaneNode::Leaf(tab_id) => ids.push(*tab_id),
        PaneNode::Split { first, second, .. } => {
            collect_tab_ids(first, ids);
            collect_tab_ids(second, ids);
        }
    }
}

fn first_tab_id(node: &PaneNode) -> Uuid {
    match node {
        PaneNode::Leaf(tab_id) => *tab_id,
        PaneNode::Split { first, .. } => first_tab_id(first),
    }
}

fn collect_placements(
    node: &PaneNode,
    rect: PaneRect,
    focused_tab_id: Uuid,
    placements: &mut Vec<PanePlacement>,
) {
    match node {
        PaneNode::Leaf(tab_id) => placements.push(PanePlacement {
            tab_id: *tab_id,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            focused: *tab_id == focused_tab_id,
        }),
        PaneNode::Split {
            axis,
            first,
            second,
        } => {
            let (first_rect, second_rect) = match axis {
                SplitAxis::Columns => (
                    PaneRect {
                        width: rect.width / 2.0,
                        ..rect
                    },
                    PaneRect {
                        x: rect.x + rect.width / 2.0,
                        width: rect.width / 2.0,
                        ..rect
                    },
                ),
                SplitAxis::Rows => (
                    PaneRect {
                        height: rect.height / 2.0,
                        ..rect
                    },
                    PaneRect {
                        y: rect.y + rect.height / 2.0,
                        height: rect.height / 2.0,
                        ..rect
                    },
                ),
            };
            collect_placements(first, first_rect, focused_tab_id, placements);
            collect_placements(second, second_rect, focused_tab_id, placements);
        }
    }
}

fn replace_leaf_with_split(
    node: &mut PaneNode,
    source: Uuid,
    direction: PaneDirection,
    new_tab_id: Uuid,
) -> bool {
    match node {
        PaneNode::Leaf(tab_id) if *tab_id == source => {
            let existing = PaneNode::Leaf(*tab_id);
            let created = PaneNode::Leaf(new_tab_id);
            let (first, second) = if direction.is_before() {
                (created, existing)
            } else {
                (existing, created)
            };
            *node = PaneNode::Split {
                axis: direction.split_axis(),
                first: Box::new(first),
                second: Box::new(second),
            };
            true
        }
        PaneNode::Leaf(_) => false,
        PaneNode::Split { first, second, .. } => {
            replace_leaf_with_split(first, source, direction, new_tab_id)
                || replace_leaf_with_split(second, source, direction, new_tab_id)
        }
    }
}

fn remove_leaf(node: PaneNode, tab_id: Uuid) -> Option<PaneNode> {
    match node {
        PaneNode::Leaf(candidate) => (candidate != tab_id).then_some(PaneNode::Leaf(candidate)),
        PaneNode::Split {
            axis,
            first,
            second,
        } => {
            let first = remove_leaf(*first, tab_id);
            let second = remove_leaf(*second, tab_id);
            match (first, second) {
                (Some(first), Some(second)) => Some(PaneNode::Split {
                    axis,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                (None, None) => None,
            }
        }
    }
}

fn directional_distance(
    current: PaneRect,
    candidate: PaneRect,
    direction: PaneDirection,
) -> Option<f32> {
    const EPSILON: f32 = 0.000_1;
    let (gap, overlap, center_offset) = match direction {
        PaneDirection::Left if candidate.center_x() < current.center_x() - EPSILON => (
            (current.x - candidate.right()).max(0.0),
            vertical_overlap(current, candidate),
            (current.center_y() - candidate.center_y()).abs(),
        ),
        PaneDirection::Right if candidate.center_x() > current.center_x() + EPSILON => (
            (candidate.x - current.right()).max(0.0),
            vertical_overlap(current, candidate),
            (current.center_y() - candidate.center_y()).abs(),
        ),
        PaneDirection::Up if candidate.center_y() < current.center_y() - EPSILON => (
            (current.y - candidate.bottom()).max(0.0),
            horizontal_overlap(current, candidate),
            (current.center_x() - candidate.center_x()).abs(),
        ),
        PaneDirection::Down if candidate.center_y() > current.center_y() + EPSILON => (
            (candidate.y - current.bottom()).max(0.0),
            horizontal_overlap(current, candidate),
            (current.center_x() - candidate.center_x()).abs(),
        ),
        _ => return None,
    };
    Some(gap * 10_000.0 - overlap * 10.0 + center_offset)
}

fn vertical_overlap(left: PaneRect, right: PaneRect) -> f32 {
    (left.bottom().min(right.bottom()) - left.y.max(right.y)).max(0.0)
}

fn horizontal_overlap(left: PaneRect, right: PaneRect) -> f32 {
    (left.right().min(right.right()) - left.x.max(right.x)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn splits_insert_the_new_terminal_on_the_requested_side() {
        let mut panes = PaneTree::new(id(1));

        assert!(panes.split_focused(PaneDirection::Right, id(2)));
        assert_eq!(panes.tab_ids(), vec![id(1), id(2)]);
        assert_eq!(panes.focused_tab_id(), id(2));

        assert!(panes.split_focused(PaneDirection::Up, id(3)));
        assert_eq!(panes.tab_ids(), vec![id(1), id(3), id(2)]);
        let placements = panes.placements();
        let upper = placements
            .iter()
            .find(|placement| placement.tab_id == id(3))
            .expect("new upper pane should be placed");
        let lower = placements
            .iter()
            .find(|placement| placement.tab_id == id(2))
            .expect("previous pane should remain placed");
        assert!(upper.y < lower.y);
    }

    #[test]
    fn focuses_directional_neighbor_after_nested_splits() {
        let mut panes = PaneTree::new(id(1));
        assert!(panes.split_focused(PaneDirection::Right, id(2)));
        assert!(panes.split_focused(PaneDirection::Down, id(3)));

        assert_eq!(panes.focus_direction(PaneDirection::Up), Some(id(2)));
        assert_eq!(panes.focus_direction(PaneDirection::Left), Some(id(1)));
        assert_eq!(panes.focus_direction(PaneDirection::Down), Some(id(3)));
        assert_eq!(panes.focus_direction(PaneDirection::Right), None);
    }

    #[test]
    fn removal_collapses_single_child_branches_and_updates_focus() {
        let mut panes = PaneTree::new(id(1));
        assert!(panes.split_focused(PaneDirection::Right, id(2)));
        assert!(panes.split_focused(PaneDirection::Down, id(3)));

        assert_eq!(panes.remove(id(2)), Some(id(3)));
        assert_eq!(panes.tab_ids(), vec![id(1), id(3)]);
        assert_eq!(panes.remove(id(1)), Some(id(3)));
        assert_eq!(panes.tab_ids(), vec![id(3)]);
        assert_eq!(panes.remove(id(3)), None);
    }

    #[test]
    fn pane_count_is_bounded() {
        let mut panes = PaneTree::new(id(1));
        for value in 2..=u128::try_from(MAX_TERMINAL_PANES).expect("pane cap fits u128") {
            assert!(panes.split_focused(PaneDirection::Right, id(value)));
        }
        assert_eq!(panes.pane_count(), MAX_TERMINAL_PANES);
        assert!(!panes.split_focused(PaneDirection::Right, id(99)));
    }
}
