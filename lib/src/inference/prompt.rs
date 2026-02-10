use crate::domain::{ArchitectureGraph, ArchitectureNode};
use crate::infrastructure::{
    C4MappingPolicy, CompletionRequest, PromptTask, ResponseFormat, SourceTreeSnapshot,
};

pub fn node_summary_request(
    graph: &ArchitectureGraph,
    node: &ArchitectureNode,
    purpose: &str,
) -> CompletionRequest {
    let system = [
        "You are Canopy, an architecture analyst that writes precise C4-aligned summaries."
            .to_string(),
        "Your output must improve a software engineer's ability to make safe changes.".to_string(),
        "Rules:".to_string(),
        "- Return plain text only. No markdown, no headings, no bullets.".to_string(),
        "- Write 2-4 concise sentences about behavioral responsibility.".to_string(),
        "- Describe collaboration boundaries using only provided dependencies/dependents."
            .to_string(),
        "- Do not invent APIs, data stores, teams, or runtime behavior not in context.".to_string(),
        "- Avoid filename-only descriptions like 'contains code for X'. State what it does."
            .to_string(),
    ];

    let mut user = Vec::new();
    user.push(format!("Repository purpose: {purpose}"));
    user.push(format!("Node: {} ({:?})", node.name, node.kind));
    user.push(format!("Path: {}", node.path.display()));

    let lineage = graph
        .lineage(&node.id)
        .into_iter()
        .filter_map(|id| graph.node(&id).map(|n| n.name.clone()))
        .collect::<Vec<_>>();
    if !lineage.is_empty() {
        user.push(format!("Lineage: {}", lineage.join(" / ")));
    }

    if !node.children.is_empty() {
        let children = node
            .children
            .iter()
            .filter_map(|id| graph.node(id).map(|n| format!("{} ({:?})", n.name, n.kind)))
            .take(16)
            .collect::<Vec<_>>();
        if !children.is_empty() {
            user.push(format!("Children: {}", children.join(", ")));
        }
    }

    if !node.dependencies.is_empty() {
        let deps = node
            .dependencies
            .iter()
            .filter_map(|id| graph.node(id).map(|n| n.name.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        user.push(format!("Depends on: {deps}"));
    }

    if !node.dependents.is_empty() {
        let deps = node
            .dependents
            .iter()
            .filter_map(|id| graph.node(id).map(|n| n.name.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        user.push(format!("Used by: {deps}"));
    }

    CompletionRequest::new(
        PromptTask::NodeSummary,
        system.join("\n"),
        user.join("\n"),
        ResponseFormat::Text,
    )
    .with_max_tokens(240)
    .with_temperature(0.1)
}

pub fn mapping_policy_request(
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    max_files_in_prompt: usize,
) -> CompletionRequest {
    let system = [
        "You are Canopy's C4 mapping planner. Convert a repository source tree into a deterministic file classification policy."
            .to_string(),
        "C4 interpretation rules:".to_string(),
        "- System: whole repository/workspace product.".to_string(),
        "- Container: deployable/runtime boundary or major subsystem boundary (service/app/worker/lib package).".to_string(),
        "- Component: cohesive behavior unit inside one container (feature, workflow, policy, engine, adapter).".to_string(),
        "- CodeUnit: source file implementation detail, never emitted in this JSON output."
            .to_string(),
        "Execution rules:".to_string(),
        "- You may use repository tools (Read, Grep, Glob, Bash) to inspect files before deciding mappings.".to_string(),
        "- Choose which files to inspect yourself, prioritizing ambiguous paths and container boundaries.".to_string(),
        "- First pass focus: implementation/runtime source first. Treat tests/fixtures/mocks as secondary unless they define core runtime behavior.".to_string(),
        "- You may inspect docs/specs/README for domain context, but do not map docs as code components.".to_string(),
        "- Build decisions from file CONTENT, not path names.".to_string(),
        "- Do not rely on file names alone when assigning component names.".to_string(),
        "- Derive component names from parsed declarations when possible (classes, modules, functions), not file stems.".to_string(),
        "Classification rules:".to_string(),
        "- Classify EVERY listed source file exactly once in mappings[].".to_string(),
        "- If include=true, container and component must be non-empty and stable across related files.".to_string(),
        "- For every included file, emit one or more contributions[] entries with evidence spans tied to real source lines.".to_string(),
        "- Emit dependencies[] as explicit component-level edges using {from:{container,component},to:{container,component},confidence,rationale}.".to_string(),
        "- include=false for noise/boilerplate-only files unless they orchestrate meaningful behavior.".to_string(),
        "- Do not create components that are only '__init__', 'mod', 'index', or filename placeholders unless file content is clearly behavioral.".to_string(),
        "- Favor fewer coherent components over many tiny filename-derived ones.".to_string(),
        "- Confidence in [0,1].".to_string(),
        "Output contract:".to_string(),
        "- Return STRICT JSON only. No prose, no markdown fences.".to_string(),
        "- Exact top-level keys: mappings, contributions, dependencies, semantic_asts, notes.".to_string(),
        "- contributions[] item shape: {\"file\":\"src/main.rs\",\"container\":\"app\",\"component\":\"entrypoint\",\"confidence\":0.93,\"rationale\":\"behavior\",\"evidence\":[{\"file\":\"src/main.rs\",\"start_line\":10,\"end_line\":22,\"excerpt\":\"fn main\",\"reason\":\"entrypoint orchestration\"}]}".to_string(),
        "- dependencies[] item shape: {\"from\":{\"container\":\"app\",\"component\":\"entrypoint\"},\"to\":{\"container\":\"app\",\"component\":\"billing\"},\"confidence\":0.82,\"rationale\":\"imports billing workflow\"}".to_string(),
        "- semantic_asts[] should summarize parsed code structure per file (functions/classes/modules with line spans).".to_string(),
        "- For each included file, semantic_asts should include the key declarations that justified component naming.".to_string(),
    ];

    let mut user = Vec::new();
    user.push(format!("Repository: {}", snapshot.repository_name));
    user.push(format!("Purpose: {purpose}"));
    user.push(format!(
        "Directories ({} shown):",
        snapshot.directories.len().min(500)
    ));
    user.push(snapshot.render_directory_preview(500));
    user.push(format!(
        "Source files ({} shown):",
        snapshot.files.len().min(max_files_in_prompt)
    ));
    user.push(snapshot.render_tree_preview(max_files_in_prompt));

    CompletionRequest::new(
        PromptTask::MappingPolicy,
        system.join("\n"),
        user.join("\n"),
        ResponseFormat::JsonObject,
    )
    .with_max_tokens(2_400)
    .with_temperature(0.0)
}

pub fn mapping_policy_repair_request(
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    max_files_in_prompt: usize,
    invalid_response: &str,
    validation_error: &str,
) -> CompletionRequest {
    let mut request = mapping_policy_request(snapshot, purpose, max_files_in_prompt);
    let clipped = clip_chars(invalid_response, 8_000);
    request.user_prompt = format!(
        "{}\n\nPrevious response was invalid.\nValidation error: {}\n\nReturn a corrected full JSON object that classifies every listed file exactly once and includes evidence-backed contributions for all included files.\nInvalid response:\n{}",
        request.user_prompt,
        validation_error.trim(),
        clipped,
    );
    request
}

pub fn mapping_policy_verification_request(
    snapshot: &SourceTreeSnapshot,
    purpose: &str,
    policy: &C4MappingPolicy,
    max_files_in_prompt: usize,
) -> CompletionRequest {
    let system = [
        "You are Canopy's architecture verifier.".to_string(),
        "Validate whether a proposed C4 mapping policy is coherent, stable, and useful for engineering navigation."
            .to_string(),
        "Verification rules:".to_string(),
        "- Policy must classify every listed source file exactly once.".to_string(),
        "- Every included file must have evidence-backed contributions with valid line spans.".to_string(),
        "- Containers should be coarse architectural boundaries, not file names.".to_string(),
        "- Components must be behavior-based and non-placeholder.".to_string(),
        "- Component names should align with parsed declarations or explicit behavioral role, not raw filename stems.".to_string(),
        "- Dependency edges must reference existing components and avoid self-cycles.".to_string(),
        "- Group related files consistently; avoid arbitrary splitting.".to_string(),
        "- Use repository tools if needed to verify ambiguous mappings.".to_string(),
        "Output contract:".to_string(),
        "- Return STRICT JSON only with shape {\"valid\":true,\"issues\":[\"...\"]}.".to_string(),
        "- Use valid=false if any significant issue exists.".to_string(),
    ];

    let mut user = Vec::new();
    user.push(format!("Repository: {}", snapshot.repository_name));
    user.push(format!("Purpose: {purpose}"));
    user.push(format!(
        "Source files ({} shown):",
        snapshot.files.len().min(max_files_in_prompt)
    ));
    user.push(snapshot.render_tree_preview(max_files_in_prompt));
    user.push("Proposed policy JSON:".to_string());
    user.push(serde_json::to_string(policy).unwrap_or_default());

    CompletionRequest::new(
        PromptTask::MappingPolicyVerify,
        system.join("\n"),
        user.join("\n"),
        ResponseFormat::JsonObject,
    )
    .with_max_tokens(480)
    .with_temperature(0.0)
}

fn clip_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let clipped: String = input.chars().take(max_chars).collect();
    format!("{clipped}\n...[truncated]...")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::domain::{ArchitectureGraph, ArchitectureNode, NodeKind};
    use crate::infrastructure::SourceTreeSnapshot;

    use super::*;

    #[test]
    fn mapping_policy_request_requires_strict_json_and_c4_rules() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "repo".to_string(),
            files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            directories: vec!["src".to_string()],
        };

        let request = mapping_policy_request(&snapshot, "Understand architecture", 200);
        assert_eq!(request.response_format, ResponseFormat::JsonObject);
        assert!(request.system_prompt.contains("STRICT JSON"));
        assert!(request.system_prompt.contains("C4 interpretation rules"));
        assert!(request.system_prompt.contains("First pass focus"));
        assert!(request.system_prompt.contains("parsed declarations"));
        assert!(request.system_prompt.contains("dependencies[]"));
        assert!(request.system_prompt.contains("__init__"));
        assert!(request.user_prompt.contains("src/main.rs"));
    }

    #[test]
    fn node_summary_request_includes_lineage_and_dependencies() {
        let root_id = "system:repo".to_string();
        let root = ArchitectureNode::new(
            root_id.clone(),
            "repo".to_string(),
            NodeKind::System,
            PathBuf::from("."),
            None,
        );
        let mut graph = ArchitectureGraph::new(root_id.clone(), root);

        let container_id = "container:src".to_string();
        graph.add_node(ArchitectureNode::new(
            container_id.clone(),
            "src".to_string(),
            NodeKind::Container,
            PathBuf::from("src"),
            Some(root_id),
        ));

        let component_id = "component:engine".to_string();
        graph.add_node(ArchitectureNode::new(
            component_id.clone(),
            "engine".to_string(),
            NodeKind::Component,
            PathBuf::from("src/engine.rs"),
            Some(container_id.clone()),
        ));
        graph.add_node(ArchitectureNode::new(
            "component:storage".to_string(),
            "storage".to_string(),
            NodeKind::Component,
            PathBuf::from("src/storage.rs"),
            Some(container_id),
        ));
        if let Some(engine) = graph.node_mut(&component_id) {
            engine.dependencies.push("component:storage".to_string());
        }
        graph.rebuild_dependents();

        let request = node_summary_request(
            &graph,
            graph.node(&component_id).expect("node"),
            "Safely modify behavior",
        );

        assert_eq!(request.response_format, ResponseFormat::Text);
        assert!(request.user_prompt.contains("Lineage:"));
        assert!(request.user_prompt.contains("Depends on: storage"));
        assert!(request.system_prompt.contains("2-4 concise sentences"));
    }

    #[test]
    fn repair_request_includes_validation_error() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "repo".to_string(),
            files: vec!["src/main.rs".to_string()],
            directories: vec!["src".to_string()],
        };

        let request = mapping_policy_repair_request(
            &snapshot,
            "Understand architecture",
            200,
            "{ bad json",
            "missing files",
        );

        assert!(request
            .user_prompt
            .contains("Validation error: missing files"));
        assert_eq!(request.response_format, ResponseFormat::JsonObject);
    }

    #[test]
    fn verification_request_requires_json_contract() {
        let snapshot = SourceTreeSnapshot {
            repository_name: "repo".to_string(),
            files: vec!["src/main.rs".to_string()],
            directories: vec!["src".to_string()],
        };
        let policy = C4MappingPolicy {
            purpose: "Understand architecture".to_string(),
            generated_at: chrono::Utc::now(),
            provider: Some("test".to_string()),
            model: Some("test-model".to_string()),
            notes: None,
            mappings: vec![],
            contributions: vec![],
            dependencies: vec![],
            semantic_asts: vec![],
        };

        let request =
            mapping_policy_verification_request(&snapshot, "Understand architecture", &policy, 100);
        assert_eq!(request.task, PromptTask::MappingPolicyVerify);
        assert_eq!(request.response_format, ResponseFormat::JsonObject);
        assert!(request.system_prompt.contains("STRICT JSON"));
    }
}
