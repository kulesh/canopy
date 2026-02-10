use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{CanopyError, Result};
use crate::infrastructure::mapping_policy::{
    normalize_path, ComponentContribution, FileMappingRule,
};

#[derive(Debug, Clone)]
pub(super) struct FilePlacement {
    pub(super) container_name: String,
    pub(super) component_name: String,
    pub(super) component_key: String,
    pub(super) component_path: PathBuf,
}

pub(super) fn policy_file_placements<'a>(
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

    let component_scope = format!(
        "{}:{}",
        container_name.to_lowercase(),
        component_name.to_lowercase()
    );
    Ok(FilePlacement {
        container_name,
        component_name: component_name.clone(),
        component_key: format!("semantic:{component_scope}"),
        component_path: rel.to_path_buf(),
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

    let component_scope = format!(
        "{}:{}",
        container_name.to_lowercase(),
        component_name.to_lowercase()
    );
    Ok(FilePlacement {
        container_name,
        component_name: component_name.clone(),
        component_key: format!("policy:{component_scope}"),
        component_path: rel.to_path_buf(),
    })
}
