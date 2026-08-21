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

use crate::{AttachmentMeta, InlineImage};
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
    pub attachments: Vec<AttachmentMeta>,
    /// Attachments carrying PR_ATTACH_CONTENT_ID, for `cid:` inlining.
    pub inline_images: Vec<InlineImage>,
}

use crate::render::MAX_INLINE_TOTAL_BYTES as MAX_INLINE_TOTAL;

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
    let mut inline_images = Vec::new();
    let mut inline_budget = MAX_INLINE_TOTAL;
    {
        let dirs = attachment_dirs(&mut cf);
        for dir in dirs {
            let prefix = format!("/{dir}");
            // An embedded message attachment stores a whole sub-message as a
            // storage under PR_ATTACH_DATA_OBJ.
            let embedded = format!("{prefix}/__substg1.0_3701000D");
            let is_message = cf.entry(&embedded).map(|e| e.is_storage()).unwrap_or(false);
            // PR_ATTACH_LONG_FILENAME, then PR_ATTACH_FILENAME (8.3), then
            // PR_ATTACH_DISPLAY_NAME; for embedded messages, their subject.
            let name = string_prop(&mut cf, &prefix, "3707")
                .or_else(|| string_prop(&mut cf, &prefix, "3704"))
                .or_else(|| string_prop(&mut cf, &prefix, "3001"))
                .or_else(|| {
                    if is_message {
                        string_prop(&mut cf, &embedded, "0037")
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| {
                    if is_message {
                        "(attached message)"
                    } else {
                        "(unnamed)"
                    }
                    .to_string()
                });
            // Size only — payloads are read solely for cid-referenced images.
            let size = cf
                .entry(format!("{prefix}/__substg1.0_37010102"))
                .map(|e| e.len())
                .unwrap_or(0);

            // PR_ATTACH_CONTENT_ID → candidate for cid: inlining.
            if !is_message {
                if let Some(cid) = string_prop(&mut cf, &prefix, "3712") {
                    let cid = cid.trim_matches(['<', '>']).to_string();
                    if !cid.is_empty() && (size as usize) <= inline_budget {
                        if let Some(data) =
                            read_stream(&mut cf, &format!("{prefix}/__substg1.0_37010102"))
                        {
                            inline_budget = inline_budget.saturating_sub(data.len());
                            // PR_ATTACH_MIME_TAG, if the producer wrote one.
                            let mime = string_prop(&mut cf, &prefix, "370E");
                            inline_images.push(InlineImage {
                                content_id: cid,
                                mime,
                                data,
                            });
                        }
                    }
                }
            }

            attachments.push(AttachmentMeta {
                name,
                size,
                is_message,
            });
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
        inline_images,
    })
}

/// Inspection data for the power-user "Technical details" panel, read straight
/// from the compound file. Kept separate from [`parse`] (which only reads what
/// the normal view renders) so the panel can show the plumbing without changing
/// the hot path.
#[derive(Default)]
pub struct MsgTechnical {
    /// PR_TRANSPORT_MESSAGE_HEADERS (0x007D) — the original RFC 822 header
    /// block, when the producer preserved it. Lets the panel show real
    /// Received/authentication headers for `.msg` too. `None` if not stored.
    pub transport_headers: Option<String>,
    /// One `(label, detail)` per root-level MAPI property stream, sorted by
    /// tag. `detail` is a short string preview for string-typed properties,
    /// otherwise the stream size.
    pub properties: Vec<(String, String)>,
    /// Number of `__recip_version1.0_*` recipient sub-storages.
    pub recipient_count: usize,
    /// Number of `__attach_version1.0_*` attachment sub-storages.
    pub attachment_count: usize,
}

/// Read the technical/inspection view of a `.msg`: transport headers plus a
/// listing of the root MAPI property streams. Hostile input degrades to an
/// empty result rather than failing.
pub fn technical(bytes: &[u8]) -> MsgTechnical {
    let mut t = MsgTechnical::default();
    let Ok(mut cf) = CompoundFile::open(Cursor::new(bytes)) else {
        return t;
    };
    t.transport_headers = string_prop(&mut cf, "", "007D");

    // Collect the root listing first (drops the read borrow) so we can re-read
    // individual streams for string previews inside the loop.
    let entries: Vec<(String, bool, u64)> = cf
        .read_root_storage()
        .map(|e| (e.name().to_string(), e.is_stream(), e.len()))
        .collect();

    let mut props: Vec<(String, String, String)> = Vec::new(); // (tag, label, detail)
    for (name, is_stream, len) in entries {
        if name.starts_with("__attach_version1.0_") {
            t.attachment_count += 1;
            continue;
        }
        if name.starts_with("__recip_version1.0_") {
            t.recipient_count += 1;
            continue;
        }
        if !is_stream {
            continue;
        }
        let Some(tag) = name.strip_prefix("__substg1.0_") else {
            continue;
        };
        // A hostile .msg can name streams with multibyte characters, so slice
        // via `get` (char-boundary-safe) rather than `tag[0..4]` which would
        // panic mid-character. A real MAPI tag is 8 ASCII hex digits.
        let (Some(id), Some(ty)) = (tag.get(0..4), tag.get(4..8)) else {
            continue;
        };
        let tyname = prop_type_name(ty);
        let pname = prop_name(id);
        let ty_label = if tyname.is_empty() {
            format!("0x{ty}")
        } else {
            tyname.to_string()
        };
        let label = if pname.is_empty() {
            format!("0x{id} ({ty_label})")
        } else {
            format!("{pname} (0x{id}, {ty_label})")
        };
        // String-typed properties get a short one-line preview; everything else
        // (binary, systime, ints) is shown by size only.
        let detail = if ty.eq_ignore_ascii_case("001F") || ty.eq_ignore_ascii_case("001E") {
            let val = string_prop(&mut cf, "", id).unwrap_or_default();
            let mut preview: String = val.chars().take(120).collect();
            preview = preview.replace(['\r', '\n', '\t'], " ");
            if val.chars().count() > 120 {
                format!("\"{preview}…\"")
            } else {
                format!("\"{preview}\"")
            }
        } else {
            format!("{len} bytes")
        };
        props.push((tag.to_string(), label, detail));
    }
    props.sort_by(|a, b| a.0.cmp(&b.0));
    t.properties = props.into_iter().map(|(_, l, d)| (l, d)).collect();
    t
}

/// Human-readable name for a well-known MAPI property id (4 hex digits, no
/// type). Empty string when unknown — the panel then shows just the hex id.
fn prop_name(id: &str) -> &'static str {
    match id.to_ascii_uppercase().as_str() {
        "0037" => "PR_SUBJECT",
        "003D" => "PR_SUBJECT_PREFIX",
        "0E1D" => "PR_NORMALIZED_SUBJECT",
        "0070" => "PR_CONVERSATION_TOPIC",
        "001A" => "PR_MESSAGE_CLASS",
        "0C1A" => "PR_SENDER_NAME",
        "0C1E" => "PR_SENDER_ADDRTYPE",
        "0C1F" => "PR_SENDER_EMAIL_ADDRESS",
        "5D01" => "PR_SENDER_SMTP_ADDRESS",
        "0E04" => "PR_DISPLAY_TO",
        "0E03" => "PR_DISPLAY_CC",
        "0E02" => "PR_DISPLAY_BCC",
        "1000" => "PR_BODY",
        "1013" => "PR_HTML",
        "1009" => "PR_RTF_COMPRESSED",
        "007D" => "PR_TRANSPORT_MESSAGE_HEADERS",
        "0039" => "PR_CLIENT_SUBMIT_TIME",
        "0E06" => "PR_MESSAGE_DELIVERY_TIME",
        "3007" => "PR_CREATION_TIME",
        "3008" => "PR_LAST_MODIFICATION_TIME",
        "0017" => "PR_IMPORTANCE",
        "0036" => "PR_SENSITIVITY",
        "3712" => "PR_ATTACH_CONTENT_ID",
        _ => "",
    }
}

/// Human-readable name for a MAPI property type (the low 4 hex digits of a
/// stream tag). Empty when unknown.
fn prop_type_name(ty: &str) -> &'static str {
    match ty.to_ascii_uppercase().as_str() {
        "001F" => "PT_UNICODE",
        "001E" => "PT_STRING8",
        "0102" => "PT_BINARY",
        "0040" => "PT_SYSTIME",
        "0003" => "PT_LONG",
        "0002" => "PT_SHORT",
        "000B" => "PT_BOOLEAN",
        "0005" => "PT_DOUBLE",
        "0014" => "PT_I8",
        "000D" => "PT_OBJECT",
        "101F" => "PT_MV_UNICODE",
        "101E" => "PT_MV_STRING8",
        "1102" => "PT_MV_BINARY",
        _ => "",
    }
}

/// Attachment storage names at the message root, sorted for a stable order
/// shared with [`crate::extract`].
pub(crate) fn attachment_dirs(cf: &mut CompoundFile<Cursor<&[u8]>>) -> Vec<String> {
    let mut dirs: Vec<String> = cf
        .read_root_storage()
        .filter(|e| e.is_storage() && e.name().starts_with("__attach_version1.0_"))
        .map(|e| e.name().to_string())
        .collect();
    dirs.sort();
    dirs
}

/// Read a whole stream, capped at [`MAX_PROP_BYTES`]. `None` if absent.
pub(crate) fn read_stream(cf: &mut CompoundFile<Cursor<&[u8]>>, path: &str) -> Option<Vec<u8>> {
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
pub(crate) fn string_prop(
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
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
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
    // Top-level messages have a 32-byte header; embedded messages (which we
    // rebuild into standalone files when opening nested attachments) use a
    // 24-byte one. Try both alignments — entries are only accepted on an
    // exact PT_SYSTIME tag match, so the wrong alignment finds nothing.
    scan_filetime_entries(bytes.get(32..)?).or_else(|| scan_filetime_entries(bytes.get(24..)?))
}

fn scan_filetime_entries(entries: &[u8]) -> Option<u64> {
    let mut submit = None;
    let mut delivery = None;
    for entry in entries.as_chunks::<16>().0 {
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
    fn prop_name_and_type_lookups() {
        assert_eq!(prop_name("0037"), "PR_SUBJECT");
        assert_eq!(prop_name("5d01"), "PR_SENDER_SMTP_ADDRESS"); // case-insensitive
        assert_eq!(prop_name("BEEF"), "");
        assert_eq!(prop_type_name("001F"), "PT_UNICODE");
        assert_eq!(prop_type_name("0102"), "PT_BINARY");
        assert_eq!(prop_type_name("9999"), "");
    }

    #[test]
    fn technical_lists_properties_from_a_real_msg() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plain-text.msg"),
        )
        .unwrap();
        let t = technical(&bytes);
        // The subject property stream is enumerated, named, and previewed.
        let subject = t
            .properties
            .iter()
            .find(|(label, _)| label.starts_with("PR_SUBJECT "))
            .expect("subject property listed");
        assert!(subject.0.contains("0x0037"));
        assert!(subject.0.contains("PT_UNICODE"));
        assert!(subject.1.starts_with('"')); // string preview, quoted
    }

    #[test]
    fn technical_on_garbage_is_empty_not_panic() {
        let t = technical(b"\xD0\xCF\x11\xE0 not a real cfb");
        assert!(t.transport_headers.is_none());
        assert!(t.properties.is_empty());
    }

    #[test]
    fn technical_survives_multibyte_stream_names() {
        // Regression for a fuzzer-found panic: a hostile .msg named a property
        // stream so a multibyte char straddled the tag slice boundary, and
        // byte-indexing `tag[0..4]` panicked. The malformed stream must be
        // skipped, not crash, while a valid property still lists.
        use std::io::Write;
        let u16le = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        let mut cf = CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        cf.create_stream("/__substg1.0_abc\u{6e65}xyz")
            .unwrap()
            .write_all(b"x")
            .unwrap();
        cf.create_stream("/__substg1.0_0037001F")
            .unwrap()
            .write_all(&u16le("Hi"))
            .unwrap();
        cf.flush().unwrap();
        let bytes = cf.into_inner().into_inner();

        let t = technical(&bytes); // must not panic
        assert!(t
            .properties
            .iter()
            .any(|(l, _)| l.starts_with("PR_SUBJECT ")));
        // The full render entry point must be panic-free on this input too.
        let _ = crate::render::render_file_to_html(&bytes, std::path::Path::new("x.msg"));
    }

    #[test]
    fn technical_surfaces_transport_headers_end_to_end() {
        // When a .msg preserves PR_TRANSPORT_MESSAGE_HEADERS (0x007D), the
        // technical view parses it AND the rendered panel shows the Received
        // delivery path + authentication results for the .msg too.
        use std::io::Write;
        let u16le = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        let mut cf = CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        // Subject so parse() accepts it as a message.
        cf.create_stream("/__substg1.0_0037001F")
            .unwrap()
            .write_all(&u16le("routed message"))
            .unwrap();
        let hdrs = "Received: from mx.example by dest.example\r\n\
                    Received: from origin.example by mx.example\r\n\
                    Authentication-Results: mx.example; spf=pass; dkim=pass\r\n\
                    Message-ID: <routed@example>\r\n";
        cf.create_stream("/__substg1.0_007D001F")
            .unwrap()
            .write_all(&u16le(hdrs))
            .unwrap();
        cf.flush().unwrap();
        let bytes = cf.into_inner().into_inner();

        let t = technical(&bytes);
        let th = t.transport_headers.expect("transport headers parsed");
        assert!(th.contains("Authentication-Results"));
        assert!(t
            .properties
            .iter()
            .any(|(l, _)| l.starts_with("PR_TRANSPORT_MESSAGE_HEADERS ")));

        let html = crate::render::render_file_to_html(&bytes, std::path::Path::new("routed.msg"));
        assert!(html.contains("spf=pass"), "auth results surfaced");
        assert!(html.contains("origin.example"), "delivery path surfaced");
        assert!(!html.contains("No Received headers"));
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
