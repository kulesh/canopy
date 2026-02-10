use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use git2::Repository as GitRepository;
use ignore::WalkBuilder;

use super::mapping_policy::{
    contributions_by_file, is_first_pass_scope_file, policy_index, C4MappingPolicy,
};
use crate::domain::{ArchitectureGraph, ArchitectureNode, NodeKind, Repository};
use crate::error::{CanopyError, Result};

mod io;
mod placements;
mod text;

use io::read_source_file;
use placements::policy_file_placements;
use text::{sanitize_id, truncate_line};

#[derive(Debug, Clone)]
pub enum RepoMapProgress {
    ScanStarted,
    FilesScanned {
        source_files: usize,
    },
    ScanCompleted {
        source_files: usize,
    },
    PolicyApplied {
        included_files: usize,
        excluded_files: usize,
    },
    ContainersBuilt {
        containers: usize,
    },
    DependencyPass {
        components: usize,
    },
    Completed {
        nodes: usize,
    },
}

#[derive(Debug, Clone)]
struct CollectedComponent {
    id: String,
    container_name: String,
    name: String,
    path: PathBuf,
    parent_id: String,
    code_units: Vec<PathBuf>,
}

fn collect_source_files(
    repository: &Repository,
    relative_files: Option<&[PathBuf]>,
    on_progress: &mut impl FnMut(RepoMapProgress),
) -> Vec<PathBuf> {
    let mut source_files = Vec::new();
    if let Some(relative_files) = relative_files {
        for relative in relative_files {
            if relative.is_absolute() {
                continue;
            }
            let full_path = repository.root.join(relative);
            if !full_path.exists() {
                continue;
            }
            if !is_first_pass_scope_file(&full_path) {
                continue;
            }
            source_files.push(full_path);
            if source_files.len().is_multiple_of(200) {
                on_progress(RepoMapProgress::FilesScanned {
                    source_files: source_files.len(),
                });
            }
        }
        return source_files;
    }

    let mut walker = WalkBuilder::new(&repository.root);
    walker.hidden(true);
    walker.git_ignore(true);
    walker.git_exclude(true);
    walker.parents(true);

    for entry in walker.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path().to_path_buf();
        if !is_first_pass_scope_file(&path) {
            continue;
        }

        source_files.push(path);
        if source_files.len().is_multiple_of(200) {
            on_progress(RepoMapProgress::FilesScanned {
                source_files: source_files.len(),
            });
        }
    }

    source_files
}

pub fn discover_repository(input_path: &Path) -> Result<Repository> {
    let absolute =
        fs::canonicalize(input_path).map_err(|source| CanopyError::io(input_path, source))?;
    let git_repo = GitRepository::discover(&absolute)?;
    let git_root = git_repo
        .workdir()
        .map(Path::to_path_buf)
        .or_else(|| git_repo.path().parent().map(Path::to_path_buf))
        .ok_or_else(|| CanopyError::InvalidInput("unable to locate git workdir".to_string()))?;

    let name = git_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repository")
        .to_string();

    let branch = git_repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(ToString::to_string));

    Ok(Repository::new(name, git_root.clone(), git_root, branch))
}

pub fn map_repository_architecture(_repository: &Repository) -> Result<ArchitectureGraph> {
    Err(CanopyError::Validation(
        "strict mapping requires a model-generated policy".to_string(),
    ))
}

pub fn map_repository_architecture_with_policy(
    repository: &Repository,
    policy: &C4MappingPolicy,
) -> Result<ArchitectureGraph> {
    map_repository_architecture_with_policy_and_progress(repository, Some(policy), |_| {})
}

pub fn map_repository_architecture_with_progress<F>(
    repository: &Repository,
    on_progress: F,
) -> Result<ArchitectureGraph>
where
    F: FnMut(RepoMapProgress),
{
    map_repository_architecture_with_policy_and_progress(repository, None, on_progress)
}

pub fn map_repository_architecture_with_policy_and_progress_for_relative_files<F>(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
    relative_files: &[PathBuf],
    on_progress: F,
) -> Result<ArchitectureGraph>
where
    F: FnMut(RepoMapProgress),
{
    map_repository_architecture_internal(repository, policy, Some(relative_files), on_progress)
}

pub fn map_repository_architecture_with_policy_and_progress<F>(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
    on_progress: F,
) -> Result<ArchitectureGraph>
where
    F: FnMut(RepoMapProgress),
{
    map_repository_architecture_internal(repository, policy, None, on_progress)
}

fn map_repository_architecture_internal<F>(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
    relative_files: Option<&[PathBuf]>,
    mut on_progress: F,
) -> Result<ArchitectureGraph>
where
    F: FnMut(RepoMapProgress),
{
    on_progress(RepoMapProgress::ScanStarted);

    let root_id = format!("system:{}", sanitize_id(&repository.name));
    let root = ArchitectureNode::new(
        root_id.clone(),
        repository.name.clone(),
        NodeKind::System,
        repository.root.clone(),
        None,
    );
    let mut graph = ArchitectureGraph::new(root_id.clone(), root);

    let source_files = collect_source_files(repository, relative_files, &mut on_progress);

    on_progress(RepoMapProgress::ScanCompleted {
        source_files: source_files.len(),
    });

    let mut container_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut collected_components: BTreeMap<String, CollectedComponent> = BTreeMap::new();

    let mut included_files = 0usize;
    let mut excluded_files = 0usize;

    let policy_lookup = policy.map(policy_index);
    let contribution_lookup = policy.map(contributions_by_file);

    for file in source_files {
        let rel = match file.strip_prefix(&repository.root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => continue,
        };

        let placements =
            if let (Some(lookup), Some(contrib_lookup)) = (&policy_lookup, &contribution_lookup) {
                policy_file_placements(&rel, lookup, contrib_lookup)?
            } else {
                return Err(CanopyError::Validation(
                    "strict mapping requires a model-generated policy".to_string(),
                ));
            };

        if placements.is_empty() {
            excluded_files += 1;
            continue;
        }
        included_files += 1;

        for placement in placements {
            container_paths
                .entry(placement.container_name.clone())
                .or_insert_with(|| repository.root.join(&placement.container_name));

            let container_id = format!("container:{}", sanitize_id(&placement.container_name));
            let component_id = format!(
                "component:{}:{}",
                sanitize_id(&placement.container_name),
                sanitize_id(&placement.component_key)
            );

            let entry = collected_components
                .entry(component_id.clone())
                .or_insert_with(|| CollectedComponent {
                    id: component_id.clone(),
                    container_name: placement.container_name.clone(),
                    name: placement.component_name.clone(),
                    path: placement.component_path.clone(),
                    parent_id: container_id,
                    code_units: Vec::new(),
                });

            entry.code_units.push(rel.clone());
        }
    }

    if policy.is_some() {
        on_progress(RepoMapProgress::PolicyApplied {
            included_files,
            excluded_files,
        });
    }

    on_progress(RepoMapProgress::ContainersBuilt {
        containers: container_paths.len(),
    });

    for (container_name, container_path) in &container_paths {
        let container_id = format!("container:{}", sanitize_id(container_name));
        let container_node = ArchitectureNode::new(
            container_id,
            container_name.clone(),
            NodeKind::Container,
            container_path.clone(),
            Some(root_id.clone()),
        );
        graph.add_node(container_node);
    }

    for component in collected_components.values() {
        let component_node = ArchitectureNode::new(
            component.id.clone(),
            component.name.clone(),
            NodeKind::Component,
            component.path.clone(),
            Some(component.parent_id.clone()),
        );
        graph.add_node(component_node);

        for code_rel in &component.code_units {
            let code_id = format!(
                "code:{}:{}",
                sanitize_id(&component.id),
                sanitize_id(code_rel.to_string_lossy().as_ref())
            );
            let mut code_node = ArchitectureNode::new(
                code_id,
                code_rel.to_string_lossy().to_string(),
                NodeKind::CodeUnit,
                code_rel.clone(),
                Some(component.id.clone()),
            );

            let full_path = repository.root.join(code_rel);
            let code = read_source_file(&full_path)?;
            code_node.summary = truncate_line(&code, 180);
            code_node.confidence = 1.0;
            graph.add_node(code_node);
        }
    }

    on_progress(RepoMapProgress::DependencyPass {
        components: graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Component)
            .count(),
    });

    if let Some(policy) = policy {
        apply_policy_dependencies(&mut graph, &collected_components, policy)?;
    }

    graph.rebuild_dependents();
    graph.validate()?;
    on_progress(RepoMapProgress::Completed {
        nodes: graph.nodes.len(),
    });
    Ok(graph)
}

fn apply_policy_dependencies(
    graph: &mut ArchitectureGraph,
    components: &BTreeMap<String, CollectedComponent>,
    policy: &C4MappingPolicy,
) -> Result<()> {
    let mut component_lookup = BTreeMap::<(String, String), String>::new();
    for component in components.values() {
        component_lookup.insert(
            (
                component.container_name.to_lowercase(),
                component.name.to_lowercase(),
            ),
            component.id.clone(),
        );
    }

    let mut edge_map = BTreeMap::<String, Vec<String>>::new();
    for dependency in &policy.dependencies {
        let from_key = (
            dependency.from.container.to_lowercase(),
            dependency.from.component.to_lowercase(),
        );
        let to_key = (
            dependency.to.container.to_lowercase(),
            dependency.to.component.to_lowercase(),
        );
        let from_id = component_lookup.get(&from_key).ok_or_else(|| {
            CanopyError::Validation(format!(
                "dependency source component not found in graph: {}::{}",
                dependency.from.container, dependency.from.component
            ))
        })?;
        let to_id = component_lookup.get(&to_key).ok_or_else(|| {
            CanopyError::Validation(format!(
                "dependency target component not found in graph: {}::{}",
                dependency.to.container, dependency.to.component
            ))
        })?;
        if from_id == to_id {
            continue;
        }
        edge_map
            .entry(from_id.clone())
            .or_default()
            .push(to_id.clone());
    }

    for (from_id, mut deps) in edge_map {
        deps.sort();
        deps.dedup();
        if let Some(node) = graph.node_mut(&from_id) {
            node.dependencies = deps;
        }
    }
    Ok(())
}
