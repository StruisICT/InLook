use crate::{AttachmentMeta, InlineImage};
use mail_parser::{Address, MessageParser, MimeHeaders, PartType};
use std::path::Path;

/// Truncate any single body part larger than this before embedding it in the
/// page. Bodies that big are pathological — typically a single huge inline
/// image or a hostile EML — and rendering them blocks WebView2 for seconds.
const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Total budget for `cid:` image data inlined into the page as base64. Beyond
/// this, further inline images are left as unresolved `cid:` refs rather than
/// letting a crafted (or just very large) email balloon the page — e.g. a
/// 20 MB message whose body embeds a huge inline image. The `.msg` reader
/// (`msg.rs`) also uses this to avoid even *reading* oversized streams.
pub(crate) const MAX_INLINE_TOTAL_BYTES: usize = 8 * 1024 * 1024;

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

    let body_text = msg.body_text(0).map(std::borrow::Cow::into_owned);

    let mut attachments: Vec<AttachmentMeta> = Vec::new();
    let mut inline_images: Vec<InlineImage> = Vec::new();
    for att in msg.attachments() {
        let is_message = matches!(att.body, PartType::Message(_));
        let name = att
            .attachment_name()
            .or_else(|| {
                att.content_type()
                    .and_then(mail_parser::ContentType::subtype)
            })
            .unwrap_or(if is_message {
                "(attached message)"
            } else {
                "(unnamed)"
            });
        if let Some(cid) = att.content_id() {
            let mime = att.content_type().map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_string(),
            });
            inline_images.push(InlineImage {
                content_id: cid.trim_matches(['<', '>']).to_string(),
                mime,
                data: att.contents().to_vec(),
            });
        }
        attachments.push(AttachmentMeta {
            name: name.to_string(),
            size: att.contents().len() as u64,
            is_message,
        });
    }

    let body_html = msg
        .body_html(0)
        .map(|h| inline_cid_images(&h, &inline_images));

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
    let body_html = m.body_html.map(|h| inline_cid_images(&h, &m.inline_images));

    page(
        &from,
        m.display_to.as_deref().unwrap_or(""),
        m.display_cc.as_deref().unwrap_or(""),
        subject,
        &date,
        body_html,
        m.body_text,
        &m.attachments,
        path,
    )
}

/// External links used by the About panel. These are our own fixed URLs — the
/// binary's navigation handler opens them in the system browser.
const COFFEE_URL: &str = "https://buymeacoffee.com/struis112";
const GITHUB_URL: &str = "https://github.com/StruisICT/InLook";
const SITE_URL: &str = "https://struisict.com";

/// Shared CSS for the app bar and the About overlay (used on every page).
const CHROME_CSS: &str = "
.appbar{display:flex;align-items:center;justify-content:space-between;padding:8px 16px;background:var(--card);border-bottom:1px solid var(--border);flex:0 0 auto;}
.appbar .brand-app{font-size:14px;font-weight:700;color:var(--accent);letter-spacing:.02em;}
.appbar .actions a{color:var(--fg);text-decoration:none;font-size:13px;padding:6px 10px;border-radius:6px;}
.appbar .actions a:hover{background:var(--card-soft);}
.about-overlay{position:fixed;inset:0;display:none;align-items:center;justify-content:center;z-index:50;}
.about-overlay:target{display:flex;}
.about-backdrop{position:absolute;inset:0;background:rgba(0,0,0,.45);}
.about-card{position:relative;background:var(--card);color:var(--fg);border:1px solid var(--border);border-radius:12px;padding:26px 28px;max-width:380px;width:90%;box-shadow:0 10px 40px rgba(0,0,0,.3);text-align:center;}
.about-card h2{margin:0 0 2px;color:var(--accent);font-size:18px;}
.about-card .ver{color:var(--muted);font-size:12px;margin-bottom:6px;}
.about-card .check-update{display:inline-block;font-size:12px;color:var(--accent);text-decoration:none;margin-bottom:14px;}
.about-card .check-update:hover{text-decoration:underline;}
.about-card p{font-size:13px;line-height:1.5;margin:0 0 18px;}
.about-card .coffee{display:inline-block;background:#ffdd00;color:#111827;font-weight:700;text-decoration:none;padding:10px 18px;border-radius:8px;margin-bottom:14px;}
.about-card .links{font-size:12px;color:var(--muted);}
.about-card .links a{color:var(--accent);text-decoration:none;}
.about-card .close{position:absolute;top:8px;right:14px;color:var(--muted);text-decoration:none;font-size:20px;line-height:1;}
";

/// The top app bar: brand plus the always-available Open / About menu items.
fn app_bar() -> &'static str {
    r##"<nav class="appbar"><span class="brand-app">&#9993; InLook</span><span class="actions"><a href="inlook://browse">Open</a> <a href="#about">About</a></span></nav>"##
}

/// The About overlay (shown via the CSS `:target` selector — no scripts). The
/// Buy Me a Coffee / GitHub / site links are opened in the system browser by
/// the navigation handler.
fn about_overlay() -> String {
    format!(
        r##"<div id="about" class="about-overlay"><a href="#" class="about-backdrop" aria-label="Close"></a><div class="about-card"><a href="#" class="close" aria-label="Close">&#215;</a><h2>InLook</h2><div class="ver">Version {ver} &middot; Free Software</div><a class="check-update" href="inlook://check-update">Check for updates</a><p>A fast, safe viewer for .eml and Outlook .msg email files, from Struis ICT.</p><a class="coffee" href="{coffee}">&#9749; Buy me a coffee</a><div class="links"><a href="{github}">GitHub</a> &middot; <a href="{site}">struisict.com</a></div></div></div>"##,
        ver = env!("CARGO_PKG_VERSION"),
        coffee = COFFEE_URL,
        github = GITHUB_URL,
        site = SITE_URL,
    )
}

/// The standalone launch screen: a drop zone that also opens the file picker
/// when clicked. Shown when InLook is started without a file argument.
pub fn render_welcome_html() -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="color-scheme" content="light dark">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; frame-src data: 'self';">
<title>InLook</title>
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
body {{ font: 14px/1.5 -apple-system, "Segoe UI", system-ui, sans-serif; background: var(--bg); color: var(--fg); margin: 0; display: flex; flex-direction: column; }}
{chrome_css}
.welcome {{ flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px; text-align: center; }}
.welcome .hero {{ font-size: 46px; margin-bottom: 6px; }}
.welcome h1 {{ font-size: 22px; color: var(--accent); margin: 0 0 6px; }}
.welcome .lead {{ color: var(--muted); margin: 0 0 26px; font-size: 14px; }}
.dropzone {{ display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; width: min(520px, 92%); min-height: 210px; border: 2px dashed var(--border); border-radius: 16px; background: var(--card); color: var(--muted); text-decoration: none; padding: 28px; }}
.dropzone:hover {{ border-color: var(--accent); background: var(--card-soft); color: var(--fg); }}
.dropzone .big {{ font-size: 15px; font-weight: 600; color: var(--fg); }}
.dropzone .sub {{ font-size: 12px; }}
</style>
</head>
<body>
{app_bar}
<div class="welcome">
  <div class="hero">&#9993;</div>
  <h1>Open an email</h1>
  <div class="lead">View .eml and Outlook .msg messages &mdash; safely, offline.</div>
  <a class="dropzone" href="inlook://browse">
    <span class="big">Drag an email here</span>
    <span class="sub">or click to browse your files</span>
  </a>
</div>
{about_overlay}
</body>
</html>"#,
        chrome_css = CHROME_CSS,
        app_bar = app_bar(),
        about_overlay = about_overlay(),
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
    attachments: &[AttachmentMeta],
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
.atts a {{ color: var(--accent); text-decoration: none; }}
.atts a:hover {{ text-decoration: underline; }}
footer {{
  padding: 8px 24px; font-size: 11px; color: var(--muted);
  background: var(--card); border-top: 1px solid var(--border);
  display: flex; justify-content: space-between; gap: 12px;
}}
footer .path {{ overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
{chrome_css}
</style>
</head>
<body>
{app_bar}
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
{about_overlay}
</body>
</html>"#,
        subject_title = html_escape::encode_text(subject),
        brand = "Struis ICT — Free Software",
        subject = html_escape::encode_text(subject),
        date = html_escape::encode_text(date),
        path_str = html_escape::encode_text(&path_str),
        path_attr = html_escape::encode_double_quoted_attribute(&path_str),
        chrome_css = CHROME_CSS,
        app_bar = app_bar(),
        about_overlay = about_overlay(),
    )
}

/// Attachment list with action links. The `inlook://save/N` and
/// `inlook://open/N` pseudo-URLs are intercepted by the binary's navigation
/// handler — no script runs in the page, so the strict no-script CSP holds.
fn attachments_section(attachments: &[AttachmentMeta]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut items = String::new();
    for (i, att) in attachments.iter().enumerate() {
        let (action, label) = if att.is_message {
            ("open", "email message — click to open".to_string())
        } else {
            ("save", format!("{} — click to save", human_bytes(att.size)))
        };
        items.push_str(&format!(
            "<li><a class=\"name\" href=\"inlook://{action}/{i}\">{}</a> <span class=\"size\">{}</span></li>",
            html_escape::encode_text(&att.name),
            label
        ));
    }
    let count = attachments.len();
    let plural = if count == 1 { "" } else { "s" };
    format!(
        r#"<details class="atts" open><summary>{count} attachment{plural}</summary><ul>{items}</ul></details>"#,
    )
}

/// Replace `cid:<content-id>` references in an HTML body with `data:` URIs
/// built from the message's own embedded parts. Keeps the no-remote-content
/// guarantee: images render without any network access, and the CSP already
/// allows `data:` images only.
fn inline_cid_images(html: &str, images: &[InlineImage]) -> String {
    let mut out = html.to_string();
    let mut budget = MAX_INLINE_TOTAL_BYTES;
    for img in images {
        if img.content_id.is_empty() || img.data.is_empty() {
            continue;
        }
        let mime = img
            .mime
            .as_deref()
            .filter(|m| m.starts_with("image/"))
            .map(str::to_string)
            .or_else(|| sniff_image_mime(&img.data).map(str::to_string));
        let Some(mime) = mime else { continue };
        // Skip images that would blow the total inline budget — they stay as
        // unresolved cid: refs (a broken image) rather than ballooning the page.
        if img.data.len() > budget {
            continue;
        }
        budget -= img.data.len();
        let uri = format!("data:{mime};base64,{}", base64(&img.data));
        out = out.replace(&format!("cid:{}", img.content_id), &uri);
    }
    out
}

/// Minimal image-type sniffer for parts without a usable declared MIME type.
fn sniff_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if data.starts_with(b"\xFF\xD8\xFF") {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

/// Standard base64 (RFC 4648, with padding). Hand-rolled to keep the
/// dependency surface of the untrusted-input path minimal.
fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18 & 63) as usize] as char);
        out.push(TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
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
    fn welcome_page_has_dropzone_browse_and_about() {
        let html = render_welcome_html();
        // Drop zone / browse action.
        assert!(html.contains(r#"href="inlook://browse""#));
        assert!(html.contains("Drag an email here"));
        // App bar + About overlay.
        assert!(html.contains(r#"class="appbar""#));
        assert!(html.contains(r#"id="about""#));
        // Clickable Buy Me a Coffee link with the real URL.
        assert!(html.contains(COFFEE_URL));
        assert!(html.contains("Buy me a coffee"));
        // On-demand update check menu item.
        assert!(html.contains(r#"href="inlook://check-update""#));
        assert!(html.contains("Check for updates"));
        // Safe by construction: no script, CSP present.
        assert!(!html.to_ascii_lowercase().contains("<script"));
        assert!(html.contains("Content-Security-Policy"));
    }

    #[test]
    fn viewer_page_carries_appbar_and_about() {
        let eml = b"From: a@b\r\nSubject: hi\r\n\r\nbody";
        let html = render_eml_to_html(eml, &PathBuf::from("t.eml"));
        assert!(html.contains(r#"class="appbar""#));
        assert!(html.contains(r#"id="about""#));
        assert!(html.contains(COFFEE_URL));
        assert!(html.contains(r#"href="inlook://browse""#));
        // The About link opens the overlay via a fragment, no script.
        assert!(html.contains(r##"href="#about""##));
        assert!(!html.to_ascii_lowercase().contains("<script"));
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

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn image_sniffer_recognizes_common_types() {
        assert_eq!(sniff_image_mime(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(sniff_image_mime(b"\xFF\xD8\xFF\xE0"), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(sniff_image_mime(b"not an image"), None);
    }

    #[test]
    fn cid_inlining_replaces_only_matching_refs() {
        let images = vec![crate::InlineImage {
            content_id: "a@b".to_string(),
            mime: Some("image/png".to_string()),
            data: vec![1, 2, 3],
        }];
        let html = r#"<img src="cid:a@b"><img src="cid:other">"#;
        let out = inline_cid_images(html, &images);
        assert!(out.contains("data:image/png;base64,AQID"));
        assert!(out.contains("cid:other"));
    }

    #[test]
    fn cid_inlining_skips_non_images() {
        let images = vec![crate::InlineImage {
            content_id: "x".to_string(),
            mime: Some("application/octet-stream".to_string()),
            data: b"MZ not an image".to_vec(),
        }];
        let out = inline_cid_images(r#"<img src="cid:x">"#, &images);
        assert!(out.contains("cid:x")); // untouched - not inlinable as an image
    }
}
