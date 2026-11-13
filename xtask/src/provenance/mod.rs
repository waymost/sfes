//! Provenance checks (PROVENANCE.md, FSD section 4).

pub(crate) mod assets;
pub(crate) mod bibliography;
pub(crate) mod commits;
pub(crate) mod lint;
pub(crate) mod scan;

/// Licenses allowed for library crates, the wheel and test assets (PROV-08, PROV-09).
pub(crate) const ALLOWED_LICENSES: &[&str] = &[
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CC0-1.0",
    "ISC",
    "MIT",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
];

/// Whether an SPDX expression permits use under `allowed` licenses: every term of an `AND`, or
/// at least one alternative of an `OR`. Parentheses are ignored, which suffices for the flat
/// expressions crates and assets use.
pub(crate) fn license_permitted(expression: &str, allowed: &dyn Fn(&str) -> bool) -> bool {
    let cleaned = expression.replace(['(', ')'], " ");
    cleaned.split(" OR ").any(|alternative| {
        alternative
            .split(" AND ")
            .map(str::trim)
            .all(|term| !term.is_empty() && allowed(term))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_list(term: &str) -> bool {
        ALLOWED_LICENSES.contains(&term)
    }

    #[test]
    fn expressions() {
        assert!(license_permitted("MIT", &in_list));
        assert!(license_permitted("MIT OR Apache-2.0", &in_list));
        assert!(license_permitted("(MIT OR GPL-3.0)", &in_list));
        assert!(license_permitted("MIT AND Zlib", &in_list));
        assert!(!license_permitted("GPL-3.0", &in_list));
        assert!(!license_permitted("MIT AND GPL-3.0", &in_list));
        assert!(!license_permitted("", &in_list));
    }
}
