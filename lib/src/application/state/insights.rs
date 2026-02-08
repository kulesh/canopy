use super::analysis::detect_cycles;
use super::AppState;

impl AppState {
    pub fn dependency_graph_ascii(&self) -> String {
        let Some(node) = self.selected_node() else {
            return String::new();
        };

        let mut out = Vec::new();
        out.push(node.name.clone());
        for dep in &node.dependencies {
            let label = self
                .graph
                .node(dep)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| dep.clone());
            out.push(format!("  ├─ uses -> {label}"));
        }

        let cycles = detect_cycles(&self.graph);
        if !cycles.is_empty() {
            out.push("Cycles:".to_string());
            for cycle in cycles {
                out.push(format!("  • {}", cycle.join(" -> ")));
            }
        }

        out.join("\n")
    }

    pub fn impact_report(&self) -> String {
        let Some(id) = self.selected_node_id() else {
            return String::new();
        };
        let impacted = self.graph.impact_set(id, self.impact_depth);
        if impacted.is_empty() {
            return "No downstream impact detected at current depth".to_string();
        }
        let mut lines = vec![format!("Potential impact ({})", impacted.len())];
        for dep in impacted {
            if let Some(node) = self.graph.node(&dep) {
                lines.push(format!("  - {}", node.name));
            }
        }
        lines.join("\n")
    }

    pub fn coverage_for_selected(&self) -> Option<f32> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        self.coverage.get(&key).map(|stats| stats.percent())
    }

    pub fn recent_change_note(&self) -> Option<String> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        let changed = self.git_signals.recent_changes.get(&key)?;
        Some(format!(
            "Last changed: {}",
            changed.format("%Y-%m-%d %H:%M UTC")
        ))
    }

    pub fn blame_note(&self) -> Option<String> {
        let node = self.selected_node()?;
        let key = node.path.to_string_lossy().to_string();
        self.git_signals
            .blame_author
            .get(&key)
            .map(|author| format!("Last author: {author}"))
    }
}
