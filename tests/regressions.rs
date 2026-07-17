//! Regression tests for fuzzer-found crashes. Each input in `tests/crashes/`
//! once panicked somewhere in the render path; rendering must stay
//! panic-free forever (release builds abort on panic, so a panic here means
//! a hostile email can crash the app outright).

use inlook::render::render_file_to_html;
use std::fs;
use std::path::Path;

#[test]
fn fuzzer_crash_inputs_do_not_panic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/crashes");
    let mut checked = 0_usize;
    for entry in fs::read_dir(dir).expect("read tests/crashes") {
        let path = entry.expect("dir entry").path();
        let bytes = fs::read(&path).expect("read crash input");
        let _ = render_file_to_html(&bytes, &path);
        checked += 1;
    }
    assert!(checked >= 1, "no crash inputs found");
}
