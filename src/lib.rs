//! InLook library crate.
//!
//! Exposes the pure, GUI-independent core of InLook so it can be tested,
//! snapshot-tested, and fuzzed without spinning up a window. The binary
//! (`src/main.rs`) is a thin shell around this: read a file, call
//! [`render::render_file_to_html`], and hand the HTML to a WebView.
//!
//! Everything here must stay free of I/O and platform glue — that lives in the
//! binary. Keeping the renderer pure is what makes the security-critical
//! message→HTML path easy to fuzz against hostile input.
#![deny(unsafe_code)]
#![warn(clippy::all)]

pub mod msg;
pub mod render;
