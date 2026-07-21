//! Realistic large-email tests — the kind of message a user opens from Outlook:
//! a small body with one or more sizeable attachments (a ~20 MB email is
//! usually 20 MB *because* of attachments, not the body). These exercise the
//! paths that only matter at scale: parsing a big file, listing large
//! attachments with correct sizes, extracting their exact bytes, and NOT
//! inlining attachment data into the page.

use inlook::extract::{extract_attachment, Extracted};
use inlook::render::render_file_to_html;
use std::io::Write;
use std::path::Path;

/// Standard base64 (RFC 4648) — for building MIME attachment bodies.
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// A blob of `total` bytes starting with `magic` (so a sniffer sees the type),
/// padded out — stands in for a real PDF/image/etc. without shipping one.
fn blob(magic: &[u8], total: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(total);
    v.extend_from_slice(magic);
    v.resize(total, b'A');
    v
}

/// Build a `multipart/mixed` .eml with an HTML body + base64 attachments.
fn eml_multipart(subject: &str, body_html: &str, atts: &[(&str, &str, &[u8])]) -> Vec<u8> {
    let boundary = "BOUNDARY_20MB";
    let mut s = format!(
        "From: sender@example.com\r\nTo: you@example.com\r\nSubject: {subject}\r\n\
         Date: Fri, 17 Jul 2026 12:00:00 +0000\r\nMIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n"
    );
    s.push_str(&format!(
        "--{boundary}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{body_html}\r\n"
    ));
    for (name, mime, bytes) in atts {
        s.push_str(&format!(
            "--{boundary}\r\nContent-Type: {mime}; name=\"{name}\"\r\n\
             Content-Disposition: attachment; filename=\"{name}\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\n"
        ));
        for line in b64(bytes).as_bytes().chunks(76) {
            s.push_str(std::str::from_utf8(line).unwrap());
            s.push_str("\r\n");
        }
    }
    s.push_str(&format!("--{boundary}--\r\n"));
    s.into_bytes()
}

#[test]
fn eml_20mb_with_attachment_lists_extracts_and_stays_small() {
    // ~15 MiB attachment → base64 pushes the .eml to ~20 MB.
    let pdf = blob(b"%PDF-1.4\n", 15 * 1024 * 1024);
    let eml = eml_multipart(
        "Quarterly report (big attachment)",
        "<p>Hi, the signed report is attached.</p>",
        &[("report.pdf", "application/pdf", &pdf)],
    );
    assert!(
        eml.len() > 19 * 1024 * 1024,
        "expected a ~20 MB email, got {} bytes",
        eml.len()
    );

    let html = render_file_to_html(&eml, Path::new("report.eml"));
    assert!(html.contains("Quarterly report (big attachment)"));
    assert!(html.contains("Hi, the signed report is attached."));
    assert!(html.contains("report.pdf"));
    assert!(html.contains("MB"), "attachment size should render in MB");

    // The 15 MiB attachment must NOT be inlined into the page — a viewer that
    // pasted it into the HTML would produce a 20 MB page.
    assert!(
        html.len() < 200 * 1024,
        "page should stay small (attachment not inlined), got {} bytes",
        html.len()
    );

    // Extraction returns the exact original bytes.
    match extract_attachment(&eml, 0) {
        Some(Extracted::File { name, data }) => {
            assert_eq!(name, "report.pdf");
            assert_eq!(data, pdf, "extracted attachment bytes must match");
        }
        other => panic!("expected a file attachment, got {:?}", other.is_some()),
    }
}

#[test]
fn eml_multiple_realistic_attachments_all_listed() {
    let pdf = blob(b"%PDF-1.4\n", 2 * 1024 * 1024);
    let jpg = blob(b"\xFF\xD8\xFF\xE0", 3 * 1024 * 1024);
    let txt = b"meeting notes".to_vec();
    let eml = eml_multipart(
        "Trip documents",
        "<p>Everything attached.</p>",
        &[
            ("itinerary.pdf", "application/pdf", &pdf),
            ("photo.jpg", "image/jpeg", &jpg),
            ("notes.txt", "text/plain", &txt),
        ],
    );
    let html = render_file_to_html(&eml, Path::new("trip.eml"));
    assert!(html.contains("3 attachments"));
    for name in ["itinerary.pdf", "photo.jpg", "notes.txt"] {
        assert!(html.contains(name), "attachment {name} not listed");
    }
    // A regular image attachment (no Content-ID) is listed, not inlined.
    assert!(
        html.len() < 200 * 1024,
        "page unexpectedly large: {}",
        html.len()
    );
}

#[test]
fn oversized_inline_cid_image_is_not_inlined() {
    // A valid-looking PNG over the 8 MiB inline budget, referenced via cid:.
    let big_png = blob(b"\x89PNG\r\n\x1a\n", 9 * 1024 * 1024);
    let boundary = "REL_BIG";
    let mut s = format!(
        "From: a@b\r\nSubject: big inline image\r\nDate: Fri, 17 Jul 2026 12:00:00 +0000\r\n\
         MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"{boundary}\"\r\n\r\n\
         --{boundary}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
         <p>Logo: <img src=\"cid:big@example\"></p>\r\n\
         --{boundary}\r\nContent-Type: image/png\r\nContent-ID: <big@example>\r\n\
         Content-Transfer-Encoding: base64\r\n\r\n"
    );
    for line in b64(&big_png).as_bytes().chunks(76) {
        s.push_str(std::str::from_utf8(line).unwrap());
        s.push_str("\r\n");
    }
    s.push_str(&format!("--{boundary}--\r\n"));

    let html = render_file_to_html(s.as_bytes(), Path::new("big.eml"));
    // Over-budget image is left as a cid: ref, not turned into a ~12 MB data URI.
    assert!(
        html.len() < 1024 * 1024,
        "oversized inline image should not be inlined; page is {} bytes",
        html.len()
    );
}

/// Build a minimal `.msg` with one large attachment stream via the `cfb` crate.
fn msg_with_attachment(subject: &str, attach_name: &str, attach: &[u8]) -> Vec<u8> {
    let u16le = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let mut cf = cfb::CompoundFile::create(std::io::Cursor::new(Vec::new())).unwrap();
    cf.create_stream("/__substg1.0_0037001F")
        .unwrap()
        .write_all(&u16le(subject))
        .unwrap();
    cf.create_stream("/__substg1.0_0C1A001F")
        .unwrap()
        .write_all(&u16le("Sender Name"))
        .unwrap();
    cf.create_stream("/__substg1.0_1000001F")
        .unwrap()
        .write_all(&u16le("See the attached file."))
        .unwrap();
    cf.create_storage("/__attach_version1.0_#00000000").unwrap();
    cf.create_stream("/__attach_version1.0_#00000000/__substg1.0_3707001F")
        .unwrap()
        .write_all(&u16le(attach_name))
        .unwrap();
    cf.create_stream("/__attach_version1.0_#00000000/__substg1.0_37010102")
        .unwrap()
        .write_all(attach)
        .unwrap();
    cf.flush().unwrap();
    cf.into_inner().into_inner()
}

#[test]
fn msg_large_attachment_lists_and_extracts() {
    let payload = blob(b"%PDF-1.4\n", 15 * 1024 * 1024);
    let msg = msg_with_attachment("Signed contract", "contract.pdf", &payload);
    assert!(msg.len() > 14 * 1024 * 1024, "expected a large .msg");

    let html = render_file_to_html(&msg, Path::new("contract.msg"));
    assert!(html.contains("Signed contract"));
    assert!(html.contains("See the attached file."));
    assert!(html.contains("contract.pdf"));
    assert!(html.contains("MB"));
    assert!(
        html.len() < 200 * 1024,
        "page should stay small, got {}",
        html.len()
    );

    match extract_attachment(&msg, 0) {
        Some(Extracted::File { name, data }) => {
            assert_eq!(name, "contract.pdf");
            assert_eq!(data, payload);
        }
        _ => panic!("expected a file attachment"),
    }
}
