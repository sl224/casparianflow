/// Returns true if the output name is already filesystem-safe.
pub fn is_safe_output_id(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Canonicalize output names into filesystem-safe identifiers.
///
/// Unsafe names are slugged and suffixed with a short hash to avoid collisions.
pub fn safe_output_id(name: &str) -> String {
    if is_safe_output_id(name) {
        return name.to_string();
    }

    let mut slug = String::with_capacity(name.len());
    let mut last_was_underscore = false;
    for ch in name.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if ch == '_' {
            '_'
        } else {
            '_'
        };

        if mapped == '_' {
            if last_was_underscore {
                continue;
            }
            last_was_underscore = true;
            slug.push('_');
        } else {
            last_was_underscore = false;
            slug.push(mapped);
        }
    }

    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "output" } else { slug };
    let hash = blake3::hash(name.as_bytes()).to_hex();
    format!("{}_{}", slug, &hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_safe(s: &str) -> bool {
        !s.is_empty()
            && s
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    #[test]
    fn safe_output_id_preserves_safe_names() {
        let name = "orders_2024";
        assert_eq!(safe_output_id(name), name);
    }

    #[test]
    fn safe_output_id_hashes_unsafe_names() {
        let name = "Orders/2024";
        let safe = safe_output_id(name);
        assert!(safe.starts_with("orders_2024_"));
        assert!(is_safe(&safe));
        assert_ne!(safe, "orders_2024");
    }

    #[test]
    fn safe_output_id_handles_empty() {
        let safe = safe_output_id("");
        assert!(is_safe(&safe));
    }
}
