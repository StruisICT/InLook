use mail_parser::{Address, MessageParser, MimeHeaders};
use std::path::Path;

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
        .map(|d| d.to_rfc822())
        .unwrap_or_else(|| "(no date)".to_string());

    let body_html = msg.body_html(0).map(|c| c.into_owned());
    let body_text = msg.body_text(0).map(|c| c.into_owned());

    let body_section = match (body_html, body_text) {
        (Some(html), _) => render_html_body(&html),
        (None, Some(text)) => format!(
            r#"<pre class="body-text">{}</pre>"#,
            html_escape::encode_text(&text)
        ),
        (None, None) => "<p class=\"empty\"><em>This message has no body.</em></p>".to_string(),
    };

    let mut attachments = String::new();
    let mut count = 0usize;
    for att in msg.attachments() {
        count += 1;
        let name = att
            .attachment_name()
            .or_else(|| att.content_type().and_then(|c| c.subtype()))
            .unwrap_or("(unnamed)");
        let size = att.contents().len();
        attachments.push_str(&format!(
            "<li><span class=\"name\">{}</span> <span class=\"size\">{}</span></li>",
            html_escape::encode_text(name),
            human_bytes(size)
        ));
    }
    let attachments_section = if count == 0 {
        String::new()
    } else {
        format!(
            r#"<details class="atts" open><summary>{count} attachment{plural}</summary><ul>{attachments}</ul><p class="hint">Use the <code>extract</code> feature on a future build to save attachments.</p></details>"#,
            plural = if count == 1 { "" } else { "s" },
        )
    };

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
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; frame-src data: 'self';">
<title>{subject_title} — InLook</title>
<style>
:root {{
  --bg: #f5f6f8; --fg: #1a1f2c; --muted: #6b7280; --accent: #2c5282;
  --border: #e2e8f0; --card: #ffffff;
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
.atts {{ padding: 12px 24px 16px; background: #f7fafc; border-top: 1px solid var(--border); }}
.atts summary {{ cursor: pointer; font-weight: 600; color: var(--accent); }}
.atts ul {{ margin: 8px 0 0; padding-left: 20px; list-style: none; }}
.atts li {{ padding: 4px 0; }}
.atts .size {{ color: var(--muted); font-size: 12px; margin-left: 8px; }}
.atts .hint {{ font-size: 11px; color: var(--muted); margin: 8px 0 0; }}
.atts code {{ background: #edf2f7; padding: 1px 5px; border-radius: 3px; }}
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
        from = from,
        to = to,
        cc_row = cc_row,
        date = html_escape::encode_text(&date),
        body_section = body_section,
        attachments_section = attachments_section,
        path_str = html_escape::encode_text(&path_str),
        path_attr = html_escape::encode_double_quoted_attribute(&path_str),
    )
}

fn render_html_body(html: &str) -> String {
    // Wrap the email's HTML in our own document with a strict CSP that blocks
    // remote loads (no tracking pixels, no remote scripts/css). Sandbox the
    // iframe so scripts and forms are disabled even if the CSP is bypassed.
    let wrapped = format!(
        r#"<!doctype html>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline' data:; font-src data:; media-src data:;">
<base target="_blank">
<style>body{{margin:0;padding:16px 24px;font:14px/1.5 -apple-system,"Segoe UI",system-ui,sans-serif;color:#1a1f2c;}} img{{max-width:100%;height:auto}}</style>
{html}"#,
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

fn format_single_addr(name: Option<&str>, address: Option<&str>) -> String {
    match (name, address) {
        (Some(n), Some(a)) if !n.is_empty() => format!(
            "{} &lt;{}&gt;",
            html_escape::encode_text(n),
            html_escape::encode_text(a)
        ),
        (_, Some(a)) => html_escape::encode_text(a).to_string(),
        (Some(n), None) => html_escape::encode_text(n).to_string(),
        (None, None) => String::new(),
    }
}

fn human_bytes(n: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let n = n as f64;
    if n >= GB {
        format!("{:.1} GB", n / GB)
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{} B", n as usize)
    }
}

fn error_page(msg: &str, path: &Path) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>Error</title>
<style>body{{font:14px -apple-system,"Segoe UI",sans-serif;padding:32px;color:#1a1f2c;background:#f5f6f8}}
h1{{color:#c53030;font-size:18px}} .path{{color:#666;font-family:monospace;font-size:12px}}</style>
<h1>Cannot display this email</h1>
<p>{}</p>
<p class="path">{}</p>"#,
        html_escape::encode_text(msg),
        html_escape::encode_text(&path.display().to_string())
    )
}

