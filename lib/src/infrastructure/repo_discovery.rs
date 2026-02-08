use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use git2::Repository as GitRepository;
use ignore::WalkBuilder;
use regex::Regex;

use super::mapping_policy::{
    contributions_by_file, is_source_file, normalize_path, policy_index, C4MappingPolicy,
    ComponentContribution, FileMappingRule,
};
use crate::domain::{ArchitectureGraph, ArchitectureNode, NodeKind, Repository};
use crate::error::{CanopyError, Result};

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
    name: String,
    path: PathBuf,
    parent_id: String,
    code_units: Vec<PathBuf>,
    aliases: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct FilePlacement {
    container_name: String,
    component_name: String,
    component_key: String,
    component_path: PathBuf,
    aliases: BTreeSet<String>,
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

pub fn map_repository_architecture(repository: &Repository) -> Result<ArchitectureGraph> {
    map_repository_architecture_with_policy_and_progress(repository, None, |_| {})
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

pub fn map_repository_architecture_with_policy_and_progress<F>(
    repository: &Repository,
    policy: Option<&C4MappingPolicy>,
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

    let mut source_files = Vec::new();
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
        if !is_source_file(&path) {
            continue;
        }

        source_files.push(path);
        if source_files.len().is_multiple_of(200) {
            on_progress(RepoMapProgress::FilesScanned {
                source_files: source_files.len(),
            });
        }
    }

    on_progress(RepoMapProgress::ScanCompleted {
        source_files: source_files.len(),
    });

    let mut container_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut collected_components: BTreeMap<String, CollectedComponent> = BTreeMap::new();
    let mut component_name_to_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut component_alias_to_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
                vec![fallback_file_placement(&rel)?]
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
                    name: placement.component_name.clone(),
                    path: placement.component_path.clone(),
                    parent_id: container_id,
                    code_units: Vec::new(),
                    aliases: BTreeSet::new(),
                });

            entry.code_units.push(rel.clone());
            entry.aliases.extend(placement.aliases);
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
            let code = fs::read_to_string(full_path).unwrap_or_default();
            code_node.summary = truncate_line(&code, 180);
            code_node.confidence = 1.0;
            graph.add_node(code_node);
        }

        component_name_to_ids
            .entry(component.name.to_lowercase())
            .or_default()
            .push(component.id.clone());

        for alias in &component.aliases {
            component_alias_to_ids
                .entry(alias.clone())
                .or_default()
                .push(component.id.clone());
        }
    }

    on_progress(RepoMapProgress::DependencyPass {
        components: graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Component)
            .count(),
    });

    infer_dependencies(
        &mut graph,
        repository,
        &component_name_to_ids,
        &component_alias_to_ids,
    )?;

    graph.rebuild_dependents();
    graph.validate()?;
    on_progress(RepoMapProgress::Completed {
        nodes: graph.nodes.len(),
    });
    Ok(graph)
}

fn infer_dependencies(
    graph: &mut ArchitectureGraph,
    repository: &Repository,
    names: &BTreeMap<String, Vec<String>>,
    aliases: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    let mut edge_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let import_re = Regex::new(r"(?i)\b(use|import|from|require|mod)\b\s+([A-Za-z0-9_:\./-]+)")
        .map_err(|err| CanopyError::Validation(format!("invalid import regex: {err}")))?;

    let component_ids: Vec<String> = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Component)
        .map(|n| n.id.clone())
        .collect();

    for component_id in &component_ids {
        let mut deps = BTreeSet::new();
        let code_child_ids: Vec<String> = graph
            .node(component_id)
            .map(|component| {
                component
                    .children
                    .iter()
                    .filter_map(|child| {
                        graph
                            .node(child)
                            .filter(|node| node.kind == NodeKind::CodeUnit)
                            .map(|node| node.id.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for code_child_id in code_child_ids {
            let Some(code_node) = graph.node(&code_child_id) else {
                continue;
            };
            let full_path = repository.root.join(&code_node.path);
            let contents = fs::read_to_string(&full_path).unwrap_or_default();

            for capture in import_re.captures_iter(&contents) {
                let token = capture
                    .get(2)
                    .map(|m| normalize_token(m.as_str()))
                    .unwrap_or_default();

                if let Some(targets) = aliases.get(&token) {
                    for target in targets {
                        if target != component_id {
                            deps.insert(target.clone());
                        }
                    }
                }

                for segment in token
                    .split(['.', ':', '/', '\\'])
                    .filter(|seg| !seg.is_empty())
                {
                    if segment.len() < 3 {
                        continue;
                    }
                    if let Some(targets) = names.get(segment) {
                        for target in targets {
                            if target != component_id {
                                deps.insert(target.clone());
                            }
                        }
                    }
                }
            }
        }

        edge_map.insert(component_id.clone(), deps);
    }

    for (id, deps) in edge_map {
        if let Some(node) = graph.node_mut(&id) {
            node.dependencies = deps.into_iter().collect();
        }
    }

    Ok(())
}

fn policy_file_placements<'a>(
    rel: &Path,
    lookup: &BTreeMap<String, &'a FileMappingRule>,
    contributions: &BTreeMap<String, Vec<&'a ComponentContribution>>,
) -> Result<Vec<FilePlacement>> {
    let key = normalize_path(&rel.to_string_lossy());
    let mapping_rule = lookup.get(&key).copied();
    let file_contributions = contributions.get(&key).cloned().unwrap_or_default();

    if !file_contributions.is_empty() {
        if mapping_rule.map(|rule| !rule.include).unwrap_or(false) {
            return Err(CanopyError::Validation(format!(
                "file {key} has include=false mapping but semantic contributions are present"
            )));
        }
        let mut placements: BTreeMap<String, FilePlacement> = BTreeMap::new();
        for contribution in file_contributions {
            let placement = placement_from_contribution(rel, contribution)?;
            placements
                .entry(placement.component_key.clone())
                .or_insert(placement);
        }
        return Ok(placements.into_values().collect());
    }

    let Some(rule) = mapping_rule else {
        return Err(CanopyError::Validation(format!(
            "mapping policy missing file: {key}"
        )));
    };

    if !rule.include {
        return Ok(Vec::new());
    }

    Ok(vec![placement_from_mapping(rel, rule)?])
}

fn placement_from_contribution(
    rel: &Path,
    contribution: &ComponentContribution,
) -> Result<FilePlacement> {
    let container_name = normalize_path(&contribution.container).replace('/', "::");
    if container_name.is_empty() {
        return Err(CanopyError::Validation(format!(
            "contribution requires non-empty container for file {}",
            contribution.file
        )));
    }
    let component_name = contribution.component.trim().to_string();
    if component_name.is_empty() {
        return Err(CanopyError::Validation(format!(
            "contribution requires non-empty component for file {}",
            contribution.file
        )));
    }

    let mut aliases = module_aliases(rel);
    aliases.insert(component_name.to_lowercase());

    Ok(FilePlacement {
        container_name,
        component_name: component_name.clone(),
        component_key: format!(
            "semantic:{}:{}",
            normalize_path(&contribution.file),
            component_name.to_lowercase()
        ),
        component_path: rel.to_path_buf(),
        aliases,
    })
}

fn placement_from_mapping(rel: &Path, rule: &FileMappingRule) -> Result<FilePlacement> {
    let container_name = rule
        .container
        .as_ref()
        .map(|v| normalize_path(v).replace('/', "::"))
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            CanopyError::Validation(format!("include=true without container for {}", rule.file))
        })?;
    let component_name = rule
        .component
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            CanopyError::Validation(format!("include=true without component for {}", rule.file))
        })?;

    let mut aliases = module_aliases(rel);
    aliases.insert(component_name.to_lowercase());

    Ok(FilePlacement {
        container_name,
        component_name: component_name.clone(),
        component_key: format!(
            "policy:{}:{}",
            rel.parent()
                .map(|v| normalize_path(&v.to_string_lossy()))
                .unwrap_or_else(|| "root".to_string()),
            component_name.to_lowercase()
        ),
        component_path: rel.to_path_buf(),
        aliases,
    })
}

fn fallback_file_placement(rel: &Path) -> Result<FilePlacement> {
    let container_name = rel
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("root")
        .to_string();

    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or_default();

    if is_initializer_file(stem, ext) {
        let fallback_parent = PathBuf::from(&container_name);
        let parent = rel.parent().unwrap_or(fallback_parent.as_path());
        let parent_name = parent
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&container_name)
            .to_string();
        let mut aliases = module_aliases(parent);
        aliases.insert(parent_name.to_lowercase());
        return Ok(FilePlacement {
            container_name,
            component_name: parent_name,
            component_key: format!("pkg:{}", parent.to_string_lossy()),
            component_path: parent.to_path_buf(),
            aliases,
        });
    }

    let component_name = stem.to_string();
    let mut aliases = module_aliases(rel);
    aliases.insert(component_name.to_lowercase());
    Ok(FilePlacement {
        container_name,
        component_name,
        component_key: format!("file:{}", rel.to_string_lossy()),
        component_path: rel.to_path_buf(),
        aliases,
    })
}

fn module_aliases(path: &Path) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    let no_ext = path.with_extension("");
    let slash = no_ext.to_string_lossy().to_lowercase();
    let dotted = slash.replace(['/', '\\'], ".");
    let scoped = slash.replace(['/', '\\'], "::");

    aliases.insert(slash);
    aliases.insert(dotted);
    aliases.insert(scoped);

    if let Some(stem) = no_ext.file_name().and_then(|s| s.to_str()) {
        aliases.insert(stem.to_lowercase());
    }

    aliases
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && !matches!(c, '_' | '.' | '/' | ':' | '-')
        })
        .to_lowercase()
}

fn is_initializer_file(stem: &str, ext: &str) -> bool {
    matches!(
        (stem, ext),
        ("__init__", "py")
            | ("mod", "rs")
            | ("index", "js")
            | ("index", "jsx")
            | ("index", "ts")
            | ("index", "tsx")
    )
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                ':'
            }
        })
        .collect()
}

fn truncate_line(contents: &str, limit: usize) -> String {
    let line = contents
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    if line.len() > limit {
        format!("{}...", &line[..limit])
    } else {
        line.to_string()
    }
}
