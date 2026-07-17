//! Outlook `.msg` parsing — pure, no I/O, no platform glue.
//!
//! A `.msg` file is an OLE Compound File ([MS-CFB]) holding MAPI properties
//! as streams ([MS-OXMSG]): variable-length properties live in streams named
//! `__substg1.0_TTTTIIII` (tag + type), fixed-length ones in
//! `__properties_version1.0`, recipients and attachments in numbered
//! sub-storages. This module reads just what the viewer shows: headers,
//! bodies, and the attachment list. Attachment *content* is never read —
//! only stream sizes.
//!
//! Untrusted-input rules: every stream read is capped, string decoding is
//! lossy, and any structural surprise degrades to `None` instead of failing.

use cfb::CompoundFile;
use std::io::{Cursor, Read};

/// Cap for any single property stream we actually read. Matches the body
/// truncation limit in `render.rs` — nothing we display needs more.
const MAX_PROP_BYTES: usize = 5 * 1024 * 1024;

/// [MS-CFB] compound-file signature. Every `.msg` starts with this.
const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// True when the bytes look like a compound file (and therefore a candidate
/// `.msg`) rather than a text-based `.eml`.
pub fn is_msg(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == CFB_MAGIC
}

/// The subset of an Outlook message the viewer renders.
pub struct Msg {
    pub sender_name: Option<String>,
    pub sender_email: Option<String>,
    pub display_to: Option<String>,
    pub display_cc: Option<String>,
    pub subject: Option<String>,
    /// RFC 822-style date string, already formatted (UTC).
    pub date: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    /// (display name, payload size in bytes)
    pub attachments: Vec<(String, u64)>,
}

/// Parse a `.msg` byte buffer. Returns `None` when the compound file cannot
/// be opened or contains nothing recognizable as a message.
pub fn parse(bytes: &[u8]) -> Option<Msg> {
    let mut cf = CompoundFile::open(Cursor::new(bytes)).ok()?;

    let subject = string_prop(&mut cf, "", "0037");
    let sender_name = string_prop(&mut cf, "", "0C1A");
    // PR_SENDER_SMTP_ADDRESS, falling back to PR_SENDER_EMAIL_ADDRESS (which
    // may be an X.500/EX address on corporate mail — still better than nothing).
    let sender_email =
        string_prop(&mut cf, "", "5D01").or_else(|| string_prop(&mut cf, "", "0C1F"));
    // PR_DISPLAY_TO / PR_DISPLAY_CC are the ready-made recipient lines
    // Outlook itself shows — no need to walk the recipient storages.
    let display_to = string_prop(&mut cf, "", "0E04");
    let display_cc = string_prop(&mut cf, "", "0E03");
    let body_text = string_prop(&mut cf, "", "1000");
    // The HTML body is usually a binary (0102) stream of the raw HTML bytes;
    // some producers write it as a string property instead.
    let body_html = read_stream(&mut cf, "/__substg1.0_10130102")
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .or_else(|| string_prop(&mut cf, "", "1013"));

    let date = fixed_props_filetime(&mut cf).map(filetime_to_rfc822);

    let mut attachments = Vec::new();
    {
        let dirs: Vec<String> = cf
            .read_root_storage()
            .filter(|e| e.is_storage() && e.name().starts_with("__attach_version1.0_"))
            .map(|e| e.name().to_string())
            .collect();
        for dir in dirs {
            let prefix = format!("/{dir}");
            // PR_ATTACH_LONG_FILENAME, then PR_ATTACH_FILENAME (8.3), then
            // PR_ATTACH_DISPLAY_NAME.
            let name = string_prop(&mut cf, &prefix, "3707")
                .or_else(|| string_prop(&mut cf, &prefix, "3704"))
                .or_else(|| string_prop(&mut cf, &prefix, "3001"))
                .unwrap_or_else(|| "(unnamed)".to_string());
            // Size only — the payload is never read.
            let size = cf
                .entry(format!("{prefix}/__substg1.0_37010102"))
                .map(|e| e.len())
                .unwrap_or(0);
            attachments.push((name, size));
        }
    }

    // A compound file with none of the message streams is not a message.
    if subject.is_none() && sender_name.is_none() && display_to.is_none() && body_text.is_none() {
        return None;
    }

    Some(Msg {
        sender_name,
        sender_email,
        display_to,
        display_cc,
        subject,
        date,
        body_text,
        body_html,
        attachments,
    })
}

/// Read a whole stream, capped at [`MAX_PROP_BYTES`]. `None` if absent.
fn read_stream(cf: &mut CompoundFile<Cursor<&[u8]>>, path: &str) -> Option<Vec<u8>> {
    let stream = cf.open_stream(path).ok()?;
    let mut buf = Vec::new();
    stream
        .take(MAX_PROP_BYTES as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// Read a string property by 4-hex-digit id under `storage_prefix` ("" for
/// the message root). Tries the Unicode (001F, UTF-16LE) variant first, then
/// the legacy 8-bit (001E) one; both decode lossily.
fn string_prop(
    cf: &mut CompoundFile<Cursor<&[u8]>>,
    storage_prefix: &str,
    id: &str,
) -> Option<String> {
    if let Some(b) = read_stream(cf, &format!("{storage_prefix}/__substg1.0_{id}001F")) {
        return Some(utf16le_lossy(&b));
    }
    read_stream(cf, &format!("{storage_prefix}/__substg1.0_{id}001E"))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn utf16le_lossy(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Scan the fixed-length property stream for the message date:
/// PR_CLIENT_SUBMIT_TIME (0x0039) or PR_MESSAGE_DELIVERY_TIME (0x0E06),
/// both PT_SYSTIME (0x0040) FILETIMEs. The top-level stream starts with a
/// 32-byte header followed by 16-byte entries: u32 tag (LE: type in the low
/// word, id in the high word), u32 flags, 8-byte value.
fn fixed_props_filetime(cf: &mut CompoundFile<Cursor<&[u8]>>) -> Option<u64> {
    let bytes = read_stream(cf, "/__properties_version1.0")?;
    let mut submit = None;
    let mut delivery = None;
    for entry in bytes.get(32..)?.chunks_exact(16) {
        let tag = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
        let (prop_type, prop_id) = ((tag & 0xFFFF) as u16, (tag >> 16) as u16);
        if prop_type != 0x0040 {
            continue;
        }
        let value = u64::from_le_bytes(entry[8..16].try_into().ok()?);
        match prop_id {
            0x0039 => submit = Some(value),
            0x0E06 => delivery = Some(value),
            _ => {}
        }
    }
    submit.or(delivery)
}

/// Format a Windows FILETIME (100 ns ticks since 1601-01-01 UTC) as an
/// RFC 822-style date string. Out-of-range values return a plain fallback
/// rather than panicking on hostile input.
fn filetime_to_rfc822(ft: u64) -> String {
    const FILETIME_UNIX_EPOCH: i64 = 11_644_473_600;
    let unix = (ft / 10_000_000) as i64 - FILETIME_UNIX_EPOCH;
    if !(0..=253_402_300_799).contains(&unix) {
        // Before 1970 or after year 9999 — hostile or corrupt.
        return "(invalid date)".to_string();
    }
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);

    // Civil-from-days (Howard Hinnant's algorithm), valid for our range.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    let weekday = WEEKDAYS[(days.rem_euclid(7)) as usize];
    format!(
        "{weekday}, {d} {} {year} {h:02}:{m:02}:{s:02} +0000",
        MONTHS[(month - 1) as usize]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_detection() {
        assert!(is_msg(&[
            0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00, 0x00
        ]));
        assert!(!is_msg(b"From: a@b\r\n\r\nhi"));
        assert!(!is_msg(b""));
    }

    #[test]
    fn garbage_is_none() {
        assert!(parse(b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1 not a real cfb").is_none());
        assert!(parse(b"").is_none());
    }

    #[test]
    fn filetime_formatting() {
        // 2026-07-17 12:00:00 UTC
        let ft = (1_784_289_600_i64 + 11_644_473_600) as u64 * 10_000_000;
        assert_eq!(filetime_to_rfc822(ft), "Fri, 17 Jul 2026 12:00:00 +0000");
        assert_eq!(filetime_to_rfc822(0), "(invalid date)");
        assert_eq!(filetime_to_rfc822(u64::MAX), "(invalid date)");
    }
}
