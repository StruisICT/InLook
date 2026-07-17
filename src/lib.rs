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

pub mod extract;
pub mod msg;
pub mod render;

/// One attachment as shown in the viewer. `index` positions map 1:1 onto
/// [`extract::extract_attachment`], for both formats.
pub struct AttachmentMeta {
    pub name: String,
    pub size: u64,
    /// An attached email (message/rfc822 part in `.eml`, embedded message
    /// storage in `.msg`) — rendered as "open in InLook" instead of "save".
    pub is_message: bool,
}

/// An inline image candidate for `cid:` substitution: content-id (without
/// angle brackets), optional declared MIME type, and the raw bytes.
pub struct InlineImage {
    pub content_id: String,
    pub mime: Option<String>,
    pub data: Vec<u8>,
}
