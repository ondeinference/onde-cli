//! The build number shown next to the version.

/// The CFBundleVersion the release scripts compiled in through
/// `ONDE_DESKTOP_BUILD`. A plain `tauri dev` build has none, and an empty value
/// counts as none.
pub fn build_number(compiled: Option<&str>) -> Option<String> {
    compiled
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::build_number;

    #[test]
    fn unset_in_a_dev_build() {
        assert_eq!(build_number(None), None);
    }

    #[test]
    fn empty_counts_as_unset() {
        assert_eq!(build_number(Some("  ")), None);
    }

    #[test]
    fn returns_the_configured_build() {
        assert_eq!(build_number(Some("20261117.1203")).as_deref(), Some("20261117.1203"));
    }
}
