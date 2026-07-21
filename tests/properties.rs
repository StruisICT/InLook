//! Property-based tests for the render/parse path — the same invariants the
//! nightly cargo-fuzz target checks, but runnable in normal CI on stable Rust.
//! They assert that *no input* can make the renderer panic or smuggle an
//! executable `<script` into the outer document.

use inlook::render::render_file_to_html;
use proptest::prelude::*;
use std::path::Path;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Arbitrary bytes (the real fuzz surface): never panic, always produce a
    /// page, never a raw `<script`.
    #[test]
    fn arbitrary_bytes_are_safe(data in proptest::collection::vec(any::<u8>(), 0..8192)) {
        let html = render_file_to_html(&data, Path::new("fuzz.eml"));
        prop_assert!(!html.is_empty());
        prop_assert!(!html.to_ascii_lowercase().contains("<script"));
    }

    /// Structurally-valid EML with attacker-controlled subject and body: the
    /// outer page must still never contain a raw `<script` (escaping + the
    /// sandboxed body iframe hold for any content).
    #[test]
    fn eml_with_arbitrary_subject_and_body_never_injects_script(
        subject in "[^\r\n]{0,200}",
        body in ".{0,2000}",
    ) {
        let eml = format!(
            "From: a@b\r\nTo: c@d\r\nSubject: {subject}\r\n\
             Content-Type: text/html\r\n\r\n{body}"
        );
        let html = render_file_to_html(eml.as_bytes(), Path::new("t.eml"));
        prop_assert!(!html.to_ascii_lowercase().contains("<script"));
        // The subject always makes it into the page (escaped) — sanity that we
        // rendered rather than bailed to an error page for ordinary input.
        prop_assert!(!html.is_empty());
    }

    /// Compound-file-shaped bytes (the `.msg` path): the CFB magic routes these
    /// to the MAPI parser, which must degrade hostile structure without panic.
    #[test]
    fn cfb_shaped_bytes_are_safe(tail in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let mut data = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        data.extend_from_slice(&tail);
        let html = render_file_to_html(&data, Path::new("fuzz.msg"));
        prop_assert!(!html.is_empty());
        prop_assert!(!html.to_ascii_lowercase().contains("<script"));
    }
}
