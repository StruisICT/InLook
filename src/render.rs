use mail_parser::{Address, MessageParser, MimeHeaders};
use std::path::Path;

/// Truncate any single body part larger than this before embedding it in the
/// page. Bodies that big are pathological — typically a single huge inline
/// image or a hostile EML — and rendering them blocks WebView2 for seconds.
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Render any supported email file into a self-contained HTML page:
/// Outlook `.msg` when the bytes carry the compound-file signature,
/// RFC 822 `.eml` otherwise. This is the single entry point the binary
/// (and the fuzzer) should use.
pub fn render_file_to_html(bytes: &[u8], path: &Path) -> String {
    if crate::msg::is_msg(bytes) {
        render_msg_to_html(bytes, path)
    } else {
        render_eml_to_html(bytes, path)
    }
}

/// Render a parsed EML byte slice into a self-contained HTML page suitable for
/// loading into a WebView2 surface. The returned page sandboxes any embedded
/// HTML body, applies a strict CSP, and HTML-escapes every header value.
pub fn render_eml_to_html(bytes: &[u8], path: &Path) -> String {
    let Some(msg) = MessageParser::default().parse(bytes) else {
        return error_page("Could not parse this file as a valid email message.", path);
    };

    let from = format_address_field(msg.from());
    let to = format_address_field(msg.to());
    let cc = format_address_field(msg.cc());
    let subject = msg.subject().unwrap_or("(no subject)");
    let date = msg
        .date()
        .map(mail_parser::DateTime::to_rfc822)
        .unwrap_or_else(|| "(no date)".to_string());

    let body_html = msg.body_html(0).map(std::borrow::Cow::into_owned);
    let body_text = msg.body_text(0).map(std::borrow::Cow::into_owned);

    let mut attachments: Vec<(String, u64)> = Vec::new();
    for att in msg.attachments() {
        let name = att
            .attachment_name()
            .or_else(|| {
                att.content_type()
                    .and_then(mail_parser::ContentType::subtype)
            })
            .unwrap_or("(unnamed)");
        attachments.push((name.to_string(), att.contents().len() as u64));
    }

    page(
        &from,
        &to,
        &cc,
        subject,
        &date,
        body_html,
        body_text,
        &attachments,
        path,
    )
}

/// Render an Outlook `.msg` byte buffer into the same self-contained HTML
/// page. Parsing lives in [`crate::msg`]; everything that touches HTML —
/// escaping, sandboxing, CSP — is shared with the EML path via [`page`].
pub fn render_msg_to_html(bytes: &[u8], path: &Path) -> String {
    let Some(m) = crate::msg::parse(bytes) else {
        return error_page(
            "Could not parse this file as an Outlook .msg message.",
            path,
        );
    };

    let from = match (m.sender_name.as_deref(), m.sender_email.as_deref()) {
        (Some(n), Some(a)) if !n.is_empty() => format!("{n} <{a}>"),
        (_, Some(a)) => a.to_string(),
        (Some(n), None) => n.to_string(),
        (None, None) => String::new(),
    };
    let subject = m.subject.as_deref().unwrap_or("(no subject)");
    let date = m.date.unwrap_or_else(|| "(no date)".to_string());

    page(
        &from,
        m.display_to.as_deref().unwrap_or(""),
        m.display_cc.as_deref().unwrap_or(""),
        subject,
        &date,
        m.body_html,
        m.body_text,
        &m.attachments,
        path,
    )
}

/// Build the full viewer page from *raw* (unescaped) header values, the
/// optional bodies, and the attachment list. All escaping happens here so
/// every input format gets the identical security treatment.
#[allow(clippy::too_many_arguments)]
fn page(
    from: &str,
    to: &str,
    cc: &str,
    subject: &str,
    date: &str,
    body_html: Option<String>,
    body_text: Option<String>,
    attachments: &[(String, u64)],
    path: &Path,
) -> String {
    let body_section = match (body_html, body_text) {
        (Some(html), _) => render_html_body(&html),
        (None, Some(text)) => format!(
            r#"<pre class="body-text">{}</pre>"#,
            html_escape::encode_text(&truncate_lossy(&text, MAX_BODY_BYTES))
        ),
        (None, None) => "<p class=\"empty\"><em>This message has no body.</em></p>".to_string(),
    };

    let attachments_section = attachments_section(attachments);

    let from = html_escape::encode_text(from);
    let to = html_escape::encode_text(to);
    let cc = html_escape::encode_text(cc);
    let path_str = path.display().to_string();
    let cc_row = if cc.is_empty() {
        String::new()
    } else {
        format!("<tr><th>Cc</th><td>{cc}</td></tr>")
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="color-scheme" content="light dark">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; frame-src data: 'self';">
<title>{subject_title} — InLook</title>
<style>
:root {{
  --bg: #f5f6f8; --fg: #1a1f2c; --muted: #6b7280; --accent: #2c5282;
  --border: #e2e8f0; --card: #ffffff; --card-soft: #f7fafc;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #0f172a; --fg: #e2e8f0; --muted: #94a3b8; --accent: #60a5fa;
    --border: #1e293b; --card: #1a202c; --card-soft: #161e2e;
  }}
}}
* {{ box-sizing: border-box; }}
html, body {{ height: 100%; }}
body {{
  font: 14px/1.5 -apple-system, "Segoe UI", system-ui, sans-serif;
  background: var(--bg); color: var(--fg); margin: 0;
  display: flex; flex-direction: column;
}}
header {{ padding: 18px 24px; background: var(--card); border-bottom: 1px solid var(--border); }}
.brand {{
  font-size: 11px; color: var(--muted); letter-spacing: 0.08em;
  text-transform: uppercase; margin-bottom: 8px; font-weight: 600;
}}
h1 {{ margin: 0 0 14px; font-size: 19px; color: var(--accent); font-weight: 600; }}
table.headers {{ border-collapse: collapse; width: 100%; }}
table.headers th {{
  text-align: left; vertical-align: top; padding: 3px 14px 3px 0;
  color: var(--muted); font-weight: 500; width: 70px; font-size: 13px;
}}
table.headers td {{ padding: 3px 0; word-break: break-word; font-size: 13px; }}
section.body-wrap {{ flex: 1; background: var(--card); display: flex; flex-direction: column; }}
iframe.body {{ flex: 1; width: 100%; border: none; min-height: 320px; background: #fff; }}
.body-text {{
  flex: 1; padding: 24px; margin: 0; white-space: pre-wrap;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 13px;
}}
.empty {{ padding: 32px; text-align: center; color: var(--muted); }}
.atts {{ padding: 12px 24px 16px; background: var(--card-soft); border-top: 1px solid var(--border); }}
.atts summary {{ cursor: pointer; font-weight: 600; color: var(--accent); }}
.atts ul {{ margin: 8px 0 0; padding-left: 20px; list-style: none; }}
.atts li {{ padding: 4px 0; }}
.atts .size {{ color: var(--muted); font-size: 12px; margin-left: 8px; }}
footer {{
  padding: 8px 24px; font-size: 11px; color: var(--muted);
  background: var(--card); border-top: 1px solid var(--border);
  display: flex; justify-content: space-between; gap: 12px;
}}
footer .path {{ overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
</style>
</head>
<body>
<header>
  <div class="brand">{brand}</div>
  <h1>{subject}</h1>
  <table class="headers">
    <tr><th>From</th><td>{from}</td></tr>
    <tr><th>To</th><td>{to}</td></tr>
    {cc_row}
    <tr><th>Date</th><td>{date}</td></tr>
  </table>
</header>
<section class="body-wrap">{body_section}</section>
{attachments_section}
<footer>
  <span class="path" title="{path_attr}">{path_str}</span>
  <span>InLook · Free Software · Struis ICT</span>
</footer>
</body>
</html>"#,
        subject_title = html_escape::encode_text(subject),
        brand = "Struis ICT — Free Software",
        subject = html_escape::encode_text(subject),
        date = html_escape::encode_text(date),
        path_str = html_escape::encode_text(&path_str),
        path_attr = html_escape::encode_double_quoted_attribute(&path_str),
    )
}

fn attachments_section(attachments: &[(String, u64)]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for (name, size) in attachments {
        items.push_str(&format!(
            "<li><span class=\"name\">{}</span> <span class=\"size\">{}</span></li>",
            html_escape::encode_text(name),
            human_bytes(*size)
        ));
    }
    let count = attachments.len();
    let plural = if count == 1 { "" } else { "s" };
    format!(
        r#"<details class="atts" open><summary>{count} attachment{plural}</summary><ul>{items}</ul></details>"#,
    )
}

fn render_html_body(html: &str) -> String {
    // Wrap the email's HTML in our own document with a strict CSP that blocks
    // remote loads (no tracking pixels, no remote scripts/css). The iframe
    // sandbox is empty (`sandbox=""`), which disables scripts, forms, popups,
    // top-navigation, downloads, and same-origin. Defense-in-depth: if the
    // CSP is bypassed, the sandbox still contains the content.
    let safe = truncate_lossy(html, MAX_BODY_BYTES);
    let wrapped = format!(
        r#"<!doctype html>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline' data:; font-src data:; media-src data:;">
<style>body{{margin:0;padding:16px 24px;font:14px/1.5 -apple-system,"Segoe UI",system-ui,sans-serif;color:#1a1f2c;}} img{{max-width:100%;height:auto}}</style>
{safe}"#,
    );
    let escaped = html_escape::encode_double_quoted_attribute(&wrapped);
    format!(r#"<iframe class="body" sandbox="" srcdoc="{escaped}"></iframe>"#)
}

fn format_address_field(value: Option<&Address>) -> String {
    let Some(addr) = value else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(list) = addr.as_list() {
        for a in list {
            parts.push(format_single_addr(a.name.as_deref(), a.address.as_deref()));
        }
    } else if let Some(group_list) = addr.as_group() {
        for group in group_list {
            for a in &group.addresses {
                parts.push(format_single_addr(a.name.as_deref(), a.address.as_deref()));
            }
        }
    }
    parts.retain(|s| !s.is_empty());
    parts.join(", ")
}

/// Format one address as raw text ("Name <addr>"). Escaping happens later in
/// [`page`], exactly once, for the whole header value.
fn format_single_addr(name: Option<&str>, address: Option<&str>) -> String {
    match (name, address) {
        (Some(n), Some(a)) if !n.is_empty() => format!("{n} <{a}>"),
        (_, Some(a)) => a.to_string(),
        (Some(n), None) => n.to_string(),
        (None, None) => String::new(),
    }
}

fn human_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.1} KB", f / KB)
    } else {
        format!("{n} B")
    }
}

/// Truncate `s` to at most `max` bytes, preserving valid UTF-8 by stepping
/// back to the previous char boundary. Appends a notice when truncated.
fn truncate_lossy(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = String::with_capacity(cut + 64);
    out.push_str(&s[..cut]);
    out.push_str("\n[…truncated by InLook — body exceeded size limit…]");
    out
}

fn error_page(msg: &str, path: &Path) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><meta name="color-scheme" content="light dark"><title>Error</title>
<style>:root{{--bg:#f5f6f8;--fg:#1a1f2c;--err:#c53030;--muted:#666}}
@media (prefers-color-scheme: dark){{:root{{--bg:#0f172a;--fg:#e2e8f0;--err:#fc8181;--muted:#94a3b8}}}}
body{{font:14px -apple-system,"Segoe UI",sans-serif;padding:32px;color:var(--fg);background:var(--bg)}}
h1{{color:var(--err);font-size:18px}} .path{{color:var(--muted);font-family:monospace;font-size:12px}}</style>
<h1>Cannot display this email</h1>
<p>{}</p>
<p class="path">{}</p>"#,
        html_escape::encode_text(msg),
        html_escape::encode_text(&path.display().to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn human_bytes_thresholds() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn format_addr_with_name() {
        let s = format_single_addr(Some("Alice"), Some("alice@example.com"));
        assert!(s.contains("Alice"));
        assert!(s.contains("alice@example.com"));
    }

    #[test]
    fn header_values_are_escaped_in_page() {
        let eml = b"From: \"<script>\" <a@b>\r\nSubject: t\r\n\r\nbody\r\n";
        let html = render_eml_to_html(eml, &PathBuf::from("t.eml"));
        assert!(!html.to_ascii_lowercase().contains("<script"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn truncate_preserves_utf8_boundary() {
        // 'é' is 2 bytes (0xC3 0xA9). Cut at 4 should land on a boundary.
        let s = "café — déjà vu";
        let out = truncate_lossy(s, 5);
        assert!(out.contains("café"));
        assert!(out.contains("truncated by InLook"));
    }

    #[test]
    fn truncate_no_op_when_small() {
        let s = "short";
        assert_eq!(truncate_lossy(s, 100), "short");
    }

    #[test]
    fn renders_simple_eml() {
        let eml = b"From: alice@example.com\r\n\
                    To: bob@example.com\r\n\
                    Subject: Hello world\r\n\
                    Date: Fri, 09 May 2026 10:00:00 +0200\r\n\
                    \r\n\
                    body content here\r\n";
        let html = render_eml_to_html(eml, &PathBuf::from("test.eml"));
        assert!(html.contains("Hello world"));
        assert!(html.contains("alice@example.com"));
        assert!(html.contains("bob@example.com"));
        assert!(html.contains("body content here"));
        assert!(html.contains("InLook"));
    }

    #[test]
    fn html_body_is_sandboxed() {
        let eml = b"From: a@x\r\n\
                    Subject: t\r\n\
                    Content-Type: text/html\r\n\
                    \r\n\
                    <script>alert(1)</script><p>hi</p>\r\n";
        let html = render_eml_to_html(eml, &PathBuf::from("t.eml"));
        assert!(html.contains(r#"sandbox="""#));
        // The script should be inside an attribute (escaped), not a top-level tag.
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn malformed_input_does_not_panic() {
        let _ = render_file_to_html(b"\x00\xff\xfe garbage", &PathBuf::from("x.eml"));
        let _ = render_file_to_html(
            b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1 bad",
            &PathBuf::from("x.msg"),
        );
    }

    #[test]
    fn supports_system_dark_mode() {
        let eml = b"From: a@x\r\nSubject: t\r\n\r\nbody\r\n";
        let html = render_eml_to_html(eml, &PathBuf::from("t.eml"));
        assert!(html.contains(r#"name="color-scheme""#));
        assert!(html.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn dispatch_selects_by_magic() {
        // Text input goes down the EML path even with a .msg name…
        let html =
            render_file_to_html(b"From: a@b\r\nSubject: s\r\n\r\nx", &PathBuf::from("a.msg"));
        assert!(html.contains("a@b"));
        // …and CFB magic goes down the MSG path (here: unparseable → msg error page).
        let html = render_file_to_html(
            b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1 truncated",
            &PathBuf::from("a.eml"),
        );
        assert!(html.contains("Outlook .msg"));
    }
}
