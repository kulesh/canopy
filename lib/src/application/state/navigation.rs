use super::{AppState, FocusPane};

impl AppState {
    pub(super) fn scroll_or_move(&mut self, delta: isize) {
        if self.focus == FocusPane::Right {
            if delta.is_negative() {
                self.right_scroll = self.right_scroll.saturating_sub(delta.unsigned_abs());
            } else {
                self.right_scroll = self.right_scroll.saturating_add(delta as usize);
            }
        } else {
            self.move_selection(delta);
        }
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            self.selected_index = 0;
            return;
        }
        let max = self.visible.len() - 1;
        if delta.is_negative() {
            self.selected_index = self.selected_index.saturating_sub(delta.unsigned_abs());
        } else {
            self.selected_index = (self.selected_index + delta as usize).min(max);
        }
    }

    pub(super) fn navigate_up(&mut self) {
        let Some(selected_id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };
        let Some(parent_id) = self
            .graph
            .node(&selected_id)
            .and_then(|node| node.parent_id.clone())
        else {
            return;
        };
        self.jump_to(&parent_id);
    }

    pub(super) fn navigate_down(&mut self) {
        let Some(selected_id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };

        let child = self
            .graph
            .node(&selected_id)
            .and_then(|node| node.children.first().cloned());

        if let Some(next) = child {
            self.jump_to(&next);
        }
    }

    pub fn toggle_collapse(&mut self) {
        let Some(id) = self.selected_node_id().map(ToString::to_string) else {
            return;
        };
        if self.collapsed.contains(&id) {
            self.collapsed.remove(&id);
        } else {
            self.collapsed.insert(id);
        }
        self.refresh_visible();
    }

    pub fn jump_to(&mut self, node_id: &str) {
        if let Some(target) = self.visible.iter().position(|(id, _)| id == node_id) {
            self.selected_index = target;
            return;
        }

        let lineage = self.graph.lineage(node_id);
        for ancestor in lineage.iter().take(lineage.len().saturating_sub(1)) {
            self.collapsed.remove(ancestor);
        }
        self.refresh_visible();

        if let Some(target) = self.visible.iter().position(|(id, _)| id == node_id) {
            self.selected_index = target;
        }
    }
}
