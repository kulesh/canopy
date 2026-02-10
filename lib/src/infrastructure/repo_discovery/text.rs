pub(super) fn sanitize_id(value: &str) -> String {
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

pub(super) fn truncate_line(contents: &str, limit: usize) -> String {
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
