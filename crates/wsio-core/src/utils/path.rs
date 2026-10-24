use anyhow::{
    Result,
    ensure,
};

/// Resolves a namespace or HTTP request path to one leading `/` without repeated or trailing `/`.
///
/// Empty and slash-only paths become `/`. Only ASCII letters, digits, `-`, `_`, `.`, and `/`
/// are allowed, without `.` or `..` segments. Percent-encodings are rejected, not decoded.
pub fn resolve_path(path: &str) -> Result<String> {
    let mut resolved = String::new();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        ensure!(
            !matches!(segment, "." | "..")
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_')),
            "Invalid path: {path}"
        );

        resolved.push('/');
        resolved.push_str(segment);
    }

    if resolved.is_empty() {
        resolved.push('/');
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests;
