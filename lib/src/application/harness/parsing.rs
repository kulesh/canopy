use std::collections::VecDeque;

use serde::Deserialize;

use crate::error::{CanopyError, Result};

#[derive(Debug, Deserialize)]
pub(crate) struct PolicyVerificationVerdict {
    pub(crate) valid: bool,
    #[serde(default)]
    pub(crate) issues: Vec<String>,
}

pub(crate) fn summarize_issues(issues: &[String]) -> String {
    if issues.is_empty() {
        return "no issues provided".to_string();
    }
    let mut list: VecDeque<String> = issues.iter().cloned().collect();
    list.make_contiguous().sort();
    list.into_iter().take(3).collect::<Vec<_>>().join(" | ")
}

pub(crate) fn parse_verification_response(raw: &str) -> Result<PolicyVerificationVerdict> {
    let json = extract_json_object(raw).ok_or_else(|| {
        CanopyError::Validation(
            "policy verification response did not contain valid JSON".to_string(),
        )
    })?;
    let verdict: PolicyVerificationVerdict = serde_json::from_str(&json)?;
    Ok(verdict)
}

pub(crate) fn extract_json_object(raw: &str) -> Option<String> {
    let stripped = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if stripped.starts_with('{') && stripped.ends_with('}') {
        return Some(stripped.to_string());
    }

    let start = stripped.find('{')?;
    let end = stripped.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(stripped[start..=end].to_string())
}
