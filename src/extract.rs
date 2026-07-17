//! Attachment extraction — pure byte-in/byte-out, shared by both formats.
//!
//! Indices match the attachment order the renderer displays (mail-parser's
//! `attachments()` order for `.eml`, sorted attachment-storage order for
//! `.msg`), so the `inlook://save/N` and `inlook://open/N` links the page
//! emits resolve to the same attachment here.

use crate::msg::{attachment_dirs, string_prop};
use cfb::CompoundFile;
use mail_parser::{MessageParser, MimeHeaders, PartType};
use std::io::{Cursor, Read, Write};

/// An extracted attachment payload.
pub enum Extracted {
    /// A regular file attachment: suggested (unsanitized) name + bytes.
    File { name: String, data: Vec<u8> },
    /// An attached email, ready to be written out and opened in a new
    /// viewer window. `is_msg` selects the file extension.
    Message {
        name: String,
        data: Vec<u8>,
        is_msg: bool,
    },
}

/// Extract attachment `index` from a supported email file. Returns `None`
/// when the file cannot be parsed or the index is out of range.
pub fn extract_attachment(bytes: &[u8], index: usize) -> Option<Extracted> {
    if crate::msg::is_msg(bytes) {
        extract_from_msg(bytes, index)
    } else {
        extract_from_eml(bytes, index)
    }
}

fn extract_from_eml(bytes: &[u8], index: usize) -> Option<Extracted> {
    let msg = MessageParser::default().parse(bytes)?;
    let part = msg.attachments().nth(index)?;
    let name = part.attachment_name().unwrap_or("attachment").to_string();
    let data = part.contents().to_vec();
    if matches!(part.body, PartType::Message(_)) {
        Some(Extracted::Message {
            name,
            data,
            is_msg: false,
        })
    } else {
        Some(Extracted::File { name, data })
    }
}

fn extract_from_msg(bytes: &[u8], index: usize) -> Option<Extracted> {
    let mut cf = CompoundFile::open(Cursor::new(bytes)).ok()?;
    let dir = attachment_dirs(&mut cf).into_iter().nth(index)?;
    let prefix = format!("/{dir}");
    let embedded = format!("{prefix}/__substg1.0_3701000D");

    let name = string_prop(&mut cf, &prefix, "3707")
        .or_else(|| string_prop(&mut cf, &prefix, "3704"))
        .or_else(|| string_prop(&mut cf, &prefix, "3001"));

    if cf.entry(&embedded).map(|e| e.is_storage()).unwrap_or(false) {
        let name = name
            .or_else(|| string_prop(&mut cf, &embedded, "0037"))
            .unwrap_or_else(|| "attached message".to_string());
        let data = rebuild_embedded_msg(&mut cf, &embedded)?;
        return Some(Extracted::Message {
            name,
            data,
            is_msg: true,
        });
    }

    let data = read_attachment_payload(&mut cf, &prefix)?;
    Some(Extracted::File {
        name: name.unwrap_or_else(|| "attachment".to_string()),
        data,
    })
}

/// Read a full attachment payload. Unlike property reads this is not capped
/// at the 5 MiB property limit — the payload is what the user asked to save —
/// but it can never exceed the (≤ 50 MiB) input buffer.
fn read_attachment_payload(cf: &mut CompoundFile<Cursor<&[u8]>>, prefix: &str) -> Option<Vec<u8>> {
    let mut stream = cf
        .open_stream(format!("{prefix}/__substg1.0_37010102"))
        .ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Copy an embedded-message storage into a fresh standalone compound file so
/// it can be opened as a regular `.msg`. Streams are copied verbatim; the
/// embedded properties stream keeps its 24-byte header, which the parser's
/// date scan tolerates.
fn rebuild_embedded_msg(cf: &mut CompoundFile<Cursor<&[u8]>>, root: &str) -> Option<Vec<u8>> {
    // Collect the subtree first (paths + kind), then copy — avoids holding an
    // iterator borrow while reading streams.
    let mut entries: Vec<(String, bool)> = Vec::new();
    let mut pending = vec![root.to_string()];
    while let Some(dir) = pending.pop() {
        let children: Vec<(String, bool)> = cf
            .read_storage(&dir)
            .ok()?
            .map(|e| (format!("{dir}/{}", e.name()), e.is_storage()))
            .collect();
        for (path, is_storage) in children {
            entries.push((path.clone(), is_storage));
            if is_storage {
                pending.push(path);
            }
        }
    }

    let mut out = CompoundFile::create(Cursor::new(Vec::new())).ok()?;
    entries.sort_by(|a, b| a.0.cmp(&b.0)); // parents before children
    for (path, is_storage) in &entries {
        let rel = path.strip_prefix(root)?;
        if *is_storage {
            out.create_storage(rel).ok()?;
        } else {
            // Read uncapped: msg::read_stream truncates at the property cap,
            // which would silently corrupt large copied payloads. The input
            // buffer (≤ 50 MiB) bounds this anyway.
            let mut s = cf.open_stream(path).ok()?;
            let mut data = Vec::new();
            s.read_to_end(&mut data).ok()?;
            out.create_stream(rel).ok()?.write_all(&data).ok()?;
        }
    }
    out.flush().ok()?;
    Some(out.into_inner().into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn eml_file_attachment_roundtrips() {
        let bytes = fixture("attachments.eml");
        let Some(Extracted::File { name, data }) = extract_attachment(&bytes, 0) else {
            panic!("expected file attachment");
        };
        assert_eq!(name, "report.pdf");
        assert!(data.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn eml_nested_message_extracts_as_message() {
        let bytes = fixture("nested-message.eml");
        let Some(Extracted::Message { data, is_msg, .. }) = extract_attachment(&bytes, 0) else {
            panic!("expected message attachment");
        };
        assert!(!is_msg);
        let inner = String::from_utf8_lossy(&data);
        assert!(inner.contains("The original"));
        assert!(inner.contains("dave@example.com"));
    }

    #[test]
    fn msg_png_attachment_roundtrips() {
        let bytes = fixture("cid-and-nested.msg");
        let Some(Extracted::File { name, data }) = extract_attachment(&bytes, 0) else {
            panic!("expected file attachment");
        };
        assert_eq!(name, "logo.png");
        assert!(data.starts_with(b"\x89PNG"));
    }

    #[test]
    fn msg_embedded_message_rebuilds_as_openable_msg() {
        let bytes = fixture("cid-and-nested.msg");
        let Some(Extracted::Message { name, data, is_msg }) = extract_attachment(&bytes, 1) else {
            panic!("expected embedded message");
        };
        assert!(is_msg);
        assert_eq!(name, "Nested subject");
        // The rebuilt bytes must themselves parse as a .msg and render.
        assert!(crate::msg::is_msg(&data));
        let inner = crate::msg::parse(&data).expect("rebuilt msg parses");
        assert_eq!(inner.subject.as_deref(), Some("Nested subject"));
        assert!(inner
            .body_text
            .as_deref()
            .unwrap()
            .contains("nested message"));
    }

    #[test]
    fn out_of_range_and_garbage_are_none() {
        assert!(extract_attachment(&fixture("attachments.eml"), 99).is_none());
        assert!(extract_attachment(b"garbage", 0).is_none());
    }
}
