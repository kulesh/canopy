use crate::domain::{ArchitectureGraph, ArchitectureNode};
use crate::infrastructure::SourceTreeSnapshot;

pub fn node_summary_prompt(graph: &ArchitectureGraph, node: &ArchitectureNode) -> String {
    let mut lines = Vec::new();
    lines.push("You are inferring software architecture summaries.".to_string());
    lines.push("Write 1-3 concise sentences describing responsibility.".to_string());
    lines.push(format!("Node: {} ({:?})", node.name, node.kind));
    lines.push(format!("Path: {}", node.path.display()));

    if !node.dependencies.is_empty() {
        let deps = node
            .dependencies
            .iter()
            .filter_map(|id| graph.node(id).map(|n| n.name.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Depends on: {deps}"));
    }

    if !node.dependents.is_empty() {
        let deps = node
            .dependents
            .iter()
            .filter_map(|id| graph.node(id).map(|n| n.name.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Used by: {deps}"));
    }

    lines.push("Return plain text only.".to_string());
    lines.join("\n")
}

pub fn mapping_policy_prompt(
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    max_files_in_prompt: usize,
) -> String {
    let mut lines = Vec::new();
    lines.push(
        "You are a software architect generating a C4 mapping policy from a repository tree."
            .to_string(),
    );
    lines.push("Task: classify EVERY listed source file.".to_string());
    lines.push(format!("Repository: {}", snapshot.repository_name));
    lines.push(format!("Purpose: {purpose}"));
    lines.push(
        "For each file, decide include/exclude. If include=true provide container and component."
            .to_string(),
    );
    lines.push("Return STRICT JSON only with this exact shape:".to_string());
    lines.push("{\"mappings\":[{\"file\":\"src/main.rs\",\"include\":true,\"container\":\"src\",\"component\":\"entrypoint\",\"confidence\":0.93,\"rationale\":\"short reason\"}],\"notes\":\"optional\"}".to_string());
    lines.push("Rules:".to_string());
    lines.push("- Include only source files relevant to architecture understanding.".to_string());
    lines.push("- Keep containers stable and coarse (service/app/domain level).".to_string());
    lines.push(
        "- Components should represent meaningful behavior units, not filenames by default."
            .to_string(),
    );
    lines.push("- Confidence must be in [0,1].".to_string());
    lines.push("- Output no markdown fences, no commentary, JSON only.".to_string());
    lines.push(format!(
        "Source files ({} shown):",
        snapshot.files.len().min(max_files_in_prompt)
    ));
    lines.push(snapshot.render_tree_preview(max_files_in_prompt));
    lines.join("\n")
}
