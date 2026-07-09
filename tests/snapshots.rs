//! Golden-file snapshot tests for the EML → HTML renderer.
//!
//! Every `tests/fixtures/*.eml` file is rendered through
//! [`inlook::render::render_eml_to_html`] and compared byte-for-byte (after
//! newline normalisation) against `tests/snapshots/<name>.html`. Any change to
//! the rendered output — intentional or not — shows up as a diff in review.
//!
//! To (re)generate snapshots after an intentional renderer change:
//!
//! ```sh
//! INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots
//! ```
//!
//! then inspect `git diff tests/snapshots/` before committing.

use inlook::render::render_eml_to_html;
use std::fs;
use std::path::{Path, PathBuf};

/// Normalise CRLF to LF so snapshots compare identically regardless of the
/// OS or git line-ending configuration the source was checked out with.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

/// Report the first line where two strings diverge, for a readable failure.
fn first_diff_line(a: &str, b: &str) -> String {
    for (i, (la, lb)) in a.lines().zip(b.lines()).enumerate() {
        if la != lb {
            return format!(
                "first diff at line {}:\n  expected: {lb}\n  actual:   {la}",
                i + 1
            );
        }
    }
    format!(
        "line count differs: actual {} vs expected {}",
        a.lines().count(),
        b.lines().count()
    )
}

#[test]
fn rendered_fixtures_match_snapshots() {
    let update = std::env::var_os("INLOOK_UPDATE_SNAPSHOTS").is_some();
    let mut checked = 0_usize;

    let mut entries: Vec<PathBuf> = fs::read_dir(fixture_dir())
        .expect("read tests/fixtures")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "eml"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no .eml fixtures found");

    for fixture in entries {
        let name = fixture.file_stem().unwrap().to_string_lossy().into_owned();
        let bytes = fs::read(&fixture).expect("read fixture");

        // Use the bare file name as the displayed path so snapshots are
        // identical on every OS (no machine-specific absolute paths).
        let shown_path = PathBuf::from(format!("{name}.eml"));
        let html = normalize(&render_eml_to_html(&bytes, &shown_path));

        // Blanket security invariant, independent of the golden files: the
        // outer document must never contain an executable script tag, no
        // matter what the input was.
        assert!(
            !html.to_ascii_lowercase().contains("<script"),
            "{name}: rendered output contains a raw <script tag"
        );

        let snap_path = snapshot_dir().join(format!("{name}.html"));
        if update {
            fs::write(&snap_path, &html).expect("write snapshot");
        } else {
            let expected = normalize(
                &String::from_utf8(fs::read(&snap_path).unwrap_or_else(|_| {
                    panic!(
                        "missing snapshot {snap_path:?} — run \
                         `INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots` \
                         and review the diff"
                    )
                }))
                .expect("snapshot is UTF-8"),
            );
            assert!(
                html == expected,
                "{name}: rendered HTML differs from snapshot {snap_path:?}\n{}\n\
                 If the change is intentional, regenerate with \
                 `INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots` \
                 and review the diff.",
                first_diff_line(&html, &expected)
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 6,
        "expected at least 6 fixtures, checked {checked}"
    );
}
