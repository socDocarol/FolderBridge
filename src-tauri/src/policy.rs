use crate::models::{Entry, FileRules, ScanOptions};

fn extension(value: &str) -> Result<String, String> {
    let value = value.trim().trim_start_matches('.');
    if value.is_empty() || value.contains(['/', '\\']) || value == "." || value == ".." {
        return Err("Enter a valid file type extension.".into());
    }
    Ok(format!(".{}", value.to_lowercase()))
}

fn relative_rule(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains(':') {
        return Err(format!("Unsafe rule path: {value}"));
    }
    let body = normalized.strip_suffix('/').unwrap_or(&normalized);
    if body
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("Unsafe rule path: {value}"));
    }
    Ok(if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    })
}

pub fn normalize_options(options: &mut ScanOptions) -> Result<(), String> {
    options.excluded_extensions = options
        .excluded_extensions
        .iter()
        .map(|e| extension(e))
        .collect::<Result<_, _>>()?;
    options.rules.review_extensions = options
        .rules
        .review_extensions
        .iter()
        .map(|e| extension(e))
        .collect::<Result<_, _>>()?;
    options.rules.excluded_paths = options
        .rules
        .excluded_paths
        .iter()
        .map(|p| relative_rule(p))
        .collect::<Result<_, _>>()?;
    if options
        .rules
        .min_size
        .zip(options.rules.max_size)
        .is_some_and(|(min, max)| min > max)
    {
        return Err("Minimum file size exceeds maximum file size.".into());
    }
    Ok(())
}

#[derive(Default)]
pub struct RuleDecision {
    pub skip_reason: Option<String>,
    pub review_reason: String,
}

pub fn classify(entry: &Entry, options: &ScanOptions) -> RuleDecision {
    if entry.status == "error" {
        return RuleDecision::default();
    }
    let path = if cfg!(windows) {
        entry.relative_path.to_lowercase()
    } else {
        entry.relative_path.clone()
    };
    let size = entry.source_size.or(entry.destination_size);
    let rules: &FileRules = &options.rules;
    let excluded = if options.excluded_extensions.contains(&entry.extension) {
        Some(format!("Skipped file type {}", entry.extension))
    } else if rules.excluded_paths.iter().any(|rule| {
        if rule.ends_with('/') {
            path.starts_with(rule)
        } else {
            path == *rule
        }
    }) {
        Some("Skipped path rule".into())
    } else if size.is_some_and(|n| rules.min_size.is_some_and(|min| n < min)) {
        Some("Below minimum file size".into())
    } else if size.is_some_and(|n| rules.max_size.is_some_and(|max| n > max)) {
        Some("Above maximum file size".into())
    } else {
        None
    };
    if excluded.is_some() {
        return RuleDecision {
            skip_reason: excluded,
            review_reason: String::new(),
        };
    }
    let mut reasons = Vec::new();
    if rules.review_extensions.contains(&entry.extension) {
        reasons.push("File type requires review");
    }
    if size.is_some_and(|n| rules.review_above.is_some_and(|limit| n > limit)) {
        reasons.push("File size requires review");
    }
    if matches!(entry.extension.as_str(), ".accdb" | ".mdb")
        && size.is_some_and(|n| rules.review_access_above.is_some_and(|limit| n > limit))
    {
        reasons.push("Access database size requires review");
    }
    RuleDecision {
        skip_reason: None,
        review_reason: reasons.join("; "),
    }
}

pub fn migration(status: &str, rule_review: bool, rule_reason: &str) -> (String, String) {
    match status {
        "source_only" if rule_review => ("review".into(), rule_reason.into()),
        "source_only" => ("ready".into(), "Ready to copy".into()),
        "different" => (
            "review".into(),
            "Destination has a different version; keep both".into(),
        ),
        "excluded" => ("skipped".into(), "Excluded by scan rules".into()),
        "destination_only" => ("skipped".into(), "Only in destination; use recovery".into()),
        "identical" => ("skipped".into(), "Already identical".into()),
        "unverified" => ("skipped".into(), "Contents were not verified".into()),
        "inventory" => ("skipped".into(), "No destination was compared".into()),
        "error" => ("skipped".into(), "File has a scan issue".into()),
        _ => ("skipped".into(), "Not eligible for migration".into()),
    }
}
