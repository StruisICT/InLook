//! Semantic-version comparison for the optional update check. Pure and
//! testable — no I/O. Only needs to answer "is `latest` newer than what we're
//! running?" for plain `vMAJOR.MINOR.PATCH` tags (the release convention;
//! see AGENTS.md §5.1).

/// Parsed `MAJOR.MINOR.PATCH`, ignoring any `v` prefix. A pre-release suffix
/// (`-rc.1`) is recorded so it can rank *below* the same finished version, per
/// SemVer precedence.
struct Version {
    core: (u64, u64, u64),
    is_prerelease: bool,
}

fn parse(tag: &str) -> Option<Version> {
    let s = tag.trim().trim_start_matches('v');
    // Split off any build metadata (ignored) and pre-release identifier.
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((c, _)) => (c, true),
        None => (s, false),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    // Reject trailing junk like "1.2.3.4".
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        core: (major, minor, patch),
        is_prerelease: pre,
    })
}

/// Whether `latest` is a strictly newer release than `current`. Returns
/// `false` on any unparseable input — we never nag on garbage.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(l), Some(c)) = (parse(latest), parse(current)) else {
        return false;
    };
    match l.core.cmp(&c.core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // Same core: a finished release outranks a pre-release of it, but two
        // equal finished versions are not "newer".
        std::cmp::Ordering::Equal => c.is_prerelease && !l.is_prerelease,
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn detects_newer_releases() {
        assert!(is_newer("v0.9.0", "0.8.0"));
        assert!(is_newer("0.8.1", "0.8.0"));
        assert!(is_newer("1.0.0", "0.8.0"));
        assert!(is_newer("v0.8.0", "v0.7.99"));
    }

    #[test]
    fn ignores_same_or_older() {
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("v0.8.0", "v0.8.0"));
        assert!(!is_newer("0.7.0", "0.8.0"));
        assert!(!is_newer("0.8.0", "1.0.0"));
    }

    #[test]
    fn prerelease_ranks_below_finished() {
        // We run a pre-release; the finished version is an update.
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        // A pre-release of the version we already run is not an update.
        assert!(!is_newer("1.0.0-rc.2", "1.0.0"));
    }

    #[test]
    fn garbage_never_nags() {
        assert!(!is_newer("", "0.8.0"));
        assert!(!is_newer("not-a-version", "0.8.0"));
        assert!(!is_newer("0.8.0", "garbage"));
        assert!(!is_newer("1.2.3.4", "0.8.0")); // too many components
    }

    #[test]
    fn short_forms_expand() {
        assert!(is_newer("v9", "0.8.0")); // 9.0.0
        assert!(is_newer("0.9", "0.8.0")); // 0.9.0
    }
}
