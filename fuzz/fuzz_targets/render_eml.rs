//! Fuzz the security-critical EML → HTML path with arbitrary bytes.
//!
//! `render_eml_to_html` is the single funnel every untrusted input goes
//! through, so it must never panic, hang, or emit an executable tag no matter
//! what bytes it is fed. Run locally (Linux/macOS, nightly) with:
//!
//! ```sh
//! cargo +nightly fuzz run render_eml -- -max_total_time=60
//! ```
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let html = inlook::render::render_eml_to_html(data, Path::new("fuzz.eml"));

    // Invariants, not just crash-freedom:
    // 1. The renderer always produces a page.
    assert!(!html.is_empty());
    // 2. Nothing in the input may smuggle an executable tag into the outer
    //    document — every path (headers, bodies, attachment names) escapes
    //    `<`, so a literal `<script` must never appear.
    assert!(
        !html.to_ascii_lowercase().contains("<script"),
        "input injected a raw <script tag into the rendered page"
    );
});
