//! Generates the `.msg` test fixtures in `tests/fixtures/`.
//!
//! Run once (and re-run only when the fixture set changes), then commit the
//! binaries:
//!
//! ```sh
//! cargo run --example gen_msg_fixtures
//! INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots
//! ```
//!
//! Building the fixtures in code (instead of exporting from Outlook) keeps
//! them minimal, licence-clean, and exactly as hostile as we want them.

use std::io::Write;

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// A fixed FILETIME: 2026-07-17 12:00:00 UTC.
const FILETIME: u64 = (1_784_289_600 + 11_644_473_600) * 10_000_000;

/// Build the fixed-length property stream: 32-byte top-level header, then one
/// PT_SYSTIME entry for PR_CLIENT_SUBMIT_TIME (0x0039).
fn properties_stream() -> Vec<u8> {
    let mut out = vec![0u8; 32];
    let tag: u32 = (0x0039 << 16) | 0x0040;
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // flags: readable
    out.extend_from_slice(&FILETIME.to_le_bytes());
    out
}

fn write_string(
    cf: &mut cfb::CompoundFile<std::fs::File>,
    path: &str,
    value: &str,
) -> std::io::Result<()> {
    cf.create_stream(path)?.write_all(&utf16le(value))
}

fn main() -> std::io::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    // 1. Plain-text message with To/Cc and a date.
    {
        let path = dir.join("plain-text.msg");
        let mut cf = cfb::create(&path)?;
        write_string(&mut cf, "/__substg1.0_0037001F", "Quarterly numbers (msg)")?;
        write_string(&mut cf, "/__substg1.0_0C1A001F", "Alice Example")?;
        write_string(&mut cf, "/__substg1.0_5D01001F", "alice@example.com")?;
        write_string(&mut cf, "/__substg1.0_0E04001F", "Bob Example")?;
        write_string(&mut cf, "/__substg1.0_0E03001F", "Carol Example")?;
        write_string(
            &mut cf,
            "/__substg1.0_1000001F",
            "Hi Bob,\r\n\r\nSame numbers, different container.\r\n\r\nGroeten,\r\nAlice",
        )?;
        cf.create_stream("/__properties_version1.0")?
            .write_all(&properties_stream())?;
        cf.flush()?;
    }

    // 2. Hostile message: script in the HTML body, XSS in subject and sender,
    //    attachment with a hostile filename.
    {
        let path = dir.join("hostile-html-attach.msg");
        let mut cf = cfb::create(&path)?;
        write_string(
            &mut cf,
            "/__substg1.0_0037001F",
            "<script>document.title</script> hostile msg",
        )?;
        write_string(
            &mut cf,
            "/__substg1.0_0C1A001F",
            "<img src=x onerror=alert(1)>",
        )?;
        write_string(&mut cf, "/__substg1.0_0C1F001F", "evil@example.com")?;
        write_string(
            &mut cf,
            "/__substg1.0_0E04001F",
            "\"Bob\" <bob@example.com>",
        )?;
        cf.create_stream("/__substg1.0_10130102")?.write_all(
            b"<html><body><h1>Msg body</h1><script>alert(1)</script>\
              <img src=\"https://tracker.example.com/p.gif\"></body></html>",
        )?;
        cf.create_storage("/__attach_version1.0_#00000000")?;
        write_string(
            &mut cf,
            "/__attach_version1.0_#00000000/__substg1.0_3707001F",
            "<img src=x>.bin",
        )?;
        cf.create_stream("/__attach_version1.0_#00000000/__substg1.0_37010102")?
            .write_all(&[0u8; 1536])?;
        cf.flush()?;
    }

    // 3. cid: inline image + embedded (nested) message attachment.
    {
        let path = dir.join("cid-and-nested.msg");
        let mut cf = cfb::create(&path)?;
        write_string(
            &mut cf,
            "/__substg1.0_0037001F",
            "Inline image and nested message",
        )?;
        write_string(&mut cf, "/__substg1.0_0C1A001F", "Alice Example")?;
        write_string(&mut cf, "/__substg1.0_5D01001F", "alice@example.com")?;
        write_string(&mut cf, "/__substg1.0_0E04001F", "Bob Example")?;
        cf.create_stream("/__substg1.0_10130102")?
            .write_all(b"<html><body><p>Logo: <img src=\"cid:logo@example\"></p></body></html>")?;
        // Attachment 0: PNG with a content id (should inline, and be savable).
        let a0 = "/__attach_version1.0_#00000000";
        cf.create_storage(a0)?;
        write_string(&mut cf, &format!("{a0}/__substg1.0_3707001F"), "logo.png")?;
        write_string(
            &mut cf,
            &format!("{a0}/__substg1.0_3712001F"),
            "<logo@example>",
        )?;
        write_string(&mut cf, &format!("{a0}/__substg1.0_370E001F"), "image/png")?;
        cf.create_stream(format!("{a0}/__substg1.0_37010102"))?
            .write_all(PNG_1PX)?;
        // Attachment 1: embedded message (open-in-InLook flow).
        let a1 = "/__attach_version1.0_#00000001";
        cf.create_storage(a1)?;
        let nested = format!("{a1}/__substg1.0_3701000D");
        cf.create_storage(&nested)?;
        write_string(
            &mut cf,
            &format!("{nested}/__substg1.0_0037001F"),
            "Nested subject",
        )?;
        write_string(
            &mut cf,
            &format!("{nested}/__substg1.0_0C1A001F"),
            "Nested Sender",
        )?;
        write_string(
            &mut cf,
            &format!("{nested}/__substg1.0_1000001F"),
            "Body of the nested message.",
        )?;
        cf.flush()?;
    }

    println!("fixtures written to {}", dir.display());
    Ok(())
}

/// A 1×1 transparent PNG — the smallest real image for cid tests.
const PNG_1PX: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];
