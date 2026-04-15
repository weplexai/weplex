/// YAML frontmatter utilities: escaping, list parsing.

/// Escape a YAML string value: wrap in quotes if it contains special chars.
pub fn yaml_escape(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_string();
    }
    if value.contains(':')
        || value.contains('#')
        || value.contains('"')
        || value.contains('\'')
        || value.contains('\n')
        || value.contains('\t')
        || value.contains('{')
        || value.contains('}')
        || value.contains('[')
        || value.contains(']')
        || value.starts_with(' ')
        || value.ends_with(' ')
    {
        // Use double-quoted YAML string, escape internal quotes, backslashes, and control chars
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        return format!("\"{}\"", escaped);
    }
    value.to_string()
}

/// Parse a YAML value that can be either:
/// - Inline list: `[Read, Grep, Edit]` or `Read, Grep, Edit`
/// - Single value: `Read`
pub fn parse_yaml_list_value(value: &str) -> Vec<String> {
    let v = value.trim();
    if v.is_empty() {
        return Vec::new();
    }
    // Bracketed list: [a, b, c]
    if v.starts_with('[') && v.ends_with(']') {
        return v[1..v.len() - 1]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Comma-separated: a, b, c
    if v.contains(',') {
        return v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Single value
    vec![v.to_string()]
}
