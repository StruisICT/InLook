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

    println!("fixtures written to {}", dir.display());
    Ok(())
}
