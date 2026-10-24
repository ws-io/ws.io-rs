/// Normalizes a slash-separated path to one leading slash, no trailing slash,
/// and no empty segments. An empty path becomes `/`; other characters are unchanged.
pub fn normalize_path(path: &str) -> String {
    format!(
        "/{}",
        path.split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    )
}

#[cfg(test)]
mod tests;
