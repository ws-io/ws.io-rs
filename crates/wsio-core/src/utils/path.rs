use anyhow::{
    Result,
    ensure,
};

/// Normalizes a `/`-separated path to one leading `/` without repeated or trailing `/`.
///
/// Empty and slash-only paths become `/`. Other characters, including percent-encodings, are unchanged.
pub fn normalize_path(path: &str) -> String {
    format!(
        "/{}",
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// Validates that a normalized namespace path contains only ASCII letters, digits, `-`, `_`, `.`, and `/`,
/// without `.` or `..` segments.
pub fn validate_namespace_path(path: &str) -> Result<()> {
    ensure!(
        path.split('/').all(|segment| {
            !matches!(segment, "." | "..")
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'))
        }),
        "Invalid namespace path: {path}"
    );

    Ok(())
}

#[cfg(test)]
mod tests;
