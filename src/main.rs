#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]
#![warn(clippy::all)]

#[cfg(windows)]
mod registry;
#[cfg(windows)]
mod update;

use inlook::render;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const APP_NAME: &str = "InLook";
const BRAND: &str = "Free Software from Struis ICT";

/// Refuse to read files larger than this. Most email is well under 25 MiB, but
/// a message with big attachments can be much larger, so the cap is generous
/// (5 GiB) to cover those edge cases. It's still bounded to avoid trying to
/// allocate absurd amounts for a hostile or accidentally-huge file; opening a
/// multi-gigabyte message may be slow or fail on low-memory machines (the whole
/// file is read into memory), but the rendered page stays small — the body is
/// truncated to `MAX_BODY_BYTES` and attachment payloads are not inlined.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// URL the WebView loads to reach our in-memory custom-protocol handler. wry
/// maps a custom scheme to `http://<scheme>.<host>` on Windows/Android and
/// `<scheme>://<host>` elsewhere.
#[cfg(any(target_os = "windows", target_os = "android"))]
const INLOOKVIEW_URL: &str = "http://inlookview.localhost/";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const INLOOKVIEW_URL: &str = "inlookview://localhost/";

/// Events posted from the WebView's navigation / drag-drop handlers back to the
/// event loop, which owns the WebView and can swap its content.
enum UserEvent {
    /// Open the file picker (the welcome screen's Browse / Open action).
    Browse,
    /// Load a specific file (e.g. one dropped onto the window).
    OpenFile(PathBuf),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str);

    // For CLI subcommands, attach to the parent terminal so println! is visible.
    // Release builds use windows_subsystem="windows" (no console); without this,
    // --version / --help / register output goes nowhere.
    if matches!(
        cmd,
        Some("--version" | "-V" | "--help" | "-h" | "register" | "unregister")
    ) {
        attach_parent_console();
    }

    match cmd {
        Some("--version" | "-V") => {
            println!("{APP_NAME} {} — {BRAND}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        #[cfg(windows)]
        Some("register") => match registry::register() {
            Ok(()) => {
                println!("{APP_NAME} is registered as a handler for .eml, .msg, and .oft files.");
                println!("Opening Windows Settings on the {APP_NAME} page — click \"Set default\"");
                println!("(Windows 11) or pick {APP_NAME} per file type (Windows 10) to finish.");
                registry::open_default_apps_settings();
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!(
                    "Registration failed: {e}\nRun this command from an elevated (Administrator) terminal."
                );
                ExitCode::FAILURE
            }
        },
        #[cfg(windows)]
        Some("unregister") => match registry::unregister() {
            Ok(()) => {
                println!("{APP_NAME} association removed.");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Unregister failed: {e}");
                ExitCode::FAILURE
            }
        },
        Some(path) if !path.starts_with("--") => open_viewer(Some(PathBuf::from(path))),
        Some(other) => {
            eprintln!("Unknown option: {other}");
            print_help();
            ExitCode::FAILURE
        }
        // No file: show the welcome screen (drag-and-drop or browse), rather
        // than popping a file picker straight away.
        None => open_viewer(None),
    }
}

fn print_help() {
    println!("{APP_NAME} {} — {BRAND}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  inlook <file>         Open an .eml / .msg / .oft email file");
    println!("  inlook                Open a file picker");
    println!("  inlook register       Associate .eml/.msg/.oft with this viewer (admin)");
    println!("  inlook unregister     Remove the file associations (admin)");
    println!("  inlook --version");
}

/// Open the main window. With `initial = Some(path)` it renders that email
/// directly (double-click / launched-with-a-file). With `initial = None` it
/// shows the welcome screen (drag-and-drop or click to browse). Either way the
/// same window can then load further files in place (browse, or drag-drop).
fn open_viewer(initial: Option<PathBuf>) -> ExitCode {
    use std::sync::{Arc, Mutex};
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    use wry::{DragDropEvent, WebViewBuilder};

    // The document currently served by the custom protocol, and the raw bytes
    // of the currently-open email (for attachment save / nested-open). Both are
    // swapped in place when a new file is loaded.
    let doc = Arc::new(Mutex::new(Vec::<u8>::new()));
    let current_bytes: Arc<Mutex<Option<Arc<Vec<u8>>>>> = Arc::new(Mutex::new(None));

    let mut title = APP_NAME.to_string();
    match &initial {
        Some(path) => match read_eml(path) {
            Ok(b) => {
                let b = Arc::new(b);
                *doc.lock().unwrap() = render::render_file_to_html(&b, path).into_bytes();
                *current_bytes.lock().unwrap() = Some(b);
                title = window_title(path);
            }
            Err(e) => {
                show_error(&format!("Cannot open {}:\n{e}", path.display()));
                return ExitCode::FAILURE;
            }
        },
        None => *doc.lock().unwrap() = render::render_welcome_html().into_bytes(),
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = match WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 800.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            show_error(&format!("Failed to create window: {e}"));
            return ExitCode::FAILURE;
        }
    };

    // WebView2's default user-data folder sits next to the executable. Installed
    // builds live in Program Files, which standard users can't write to, so that
    // default fails with "access denied" (HRESULT 0x80070005). Point WebView2 at
    // a per-user, writable folder instead. `web_context` must outlive `build()`;
    // it lives until the (diverging) event loop, so it is never dropped early.
    let mut web_context = wry::WebContext::new(webview_data_dir());

    let doc_proto = doc.clone();
    let nav_bytes = current_bytes.clone();
    let nav_proxy = proxy.clone();
    let drop_proxy = proxy.clone();

    // Serve the current page from memory via a custom protocol instead of
    // `with_html`. On Windows `with_html` goes through WebView2's
    // NavigateToString, which caps the page at 2 MB — a large or image-heavy
    // email (inlined `cid:` images are base64, +33%) exceeds it and fails to
    // display. A custom protocol streams the response like a local server: no
    // size limit, and the HTML stays in memory so the email body never touches
    // disk. The CSP is also sent as an HTTP header, so it applies at the
    // transport layer, not only via the page's <meta> tag. Reloading the URL
    // after a swap re-requests this handler, which serves the new document.
    let webview = match WebViewBuilder::new(&window)
        .with_web_context(&mut web_context)
        .with_custom_protocol("inlookview".to_string(), move |_request| {
            let body = doc_proto.lock().unwrap().clone();
            wry::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header(
                    "Content-Security-Policy",
                    "default-src 'none'; img-src data:; style-src 'unsafe-inline'; frame-src data: 'self';",
                )
                .body(std::borrow::Cow::from(body))
                .unwrap_or_else(|_| wry::http::Response::new(std::borrow::Cow::from(Vec::new())))
        })
        .with_url(INLOOKVIEW_URL)
        .with_navigation_handler(move |url| handle_navigation(&url, &nav_bytes, &nav_proxy))
        .with_drag_drop_handler(move |event| {
            if let DragDropEvent::Drop { paths, .. } = event {
                if let Some(p) = paths.into_iter().find(|p| p.is_file()) {
                    let _ = drop_proxy.send_event(UserEvent::OpenFile(p));
                }
            }
            true // we handle drops ourselves; don't fall through to the OS
        })
        .build()
    {
        Ok(v) => v,
        Err(e) => {
            show_error(&webview_error_message(&e));
            return ExitCode::FAILURE;
        }
    };

    // On Windows, run the opt-in update check once after the window has
    // painted, so the content is visible behind any first-run consent prompt.
    #[cfg(windows)]
    let mut update_check_pending = true;

    let doc_loop = doc.clone();
    let bytes_loop = current_bytes.clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::Browse) => {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title(format!("{APP_NAME} — open email file"))
                    .add_filter("Email message", &["eml", "msg", "oft"])
                    .pick_file()
                {
                    load_file(&webview, &window, &p, &doc_loop, &bytes_loop);
                }
            }
            Event::UserEvent(UserEvent::OpenFile(p)) => {
                load_file(&webview, &window, &p, &doc_loop, &bytes_loop);
            }
            #[cfg(windows)]
            Event::RedrawEventsCleared if update_check_pending => {
                update_check_pending = false;
                update::maybe_run(true);
            }
            _ => {}
        }
    });
}

/// Window title for a given file: "name — InLook".
fn window_title(path: &Path) -> String {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("email");
    format!("{name} — {APP_NAME}")
}

/// Render `path` into the shared document and reload the WebView so it swaps to
/// the new email in place. On a read error the current view is left untouched.
fn load_file(
    webview: &wry::WebView,
    window: &tao::window::Window,
    path: &Path,
    doc: &std::sync::Mutex<Vec<u8>>,
    current_bytes: &std::sync::Mutex<Option<std::sync::Arc<Vec<u8>>>>,
) {
    match read_eml(path) {
        Ok(b) => {
            let b = std::sync::Arc::new(b);
            let html = render::render_file_to_html(&b, path);
            *doc.lock().unwrap() = html.into_bytes();
            *current_bytes.lock().unwrap() = Some(b);
            let _ = webview.load_url(INLOOKVIEW_URL);
            window.set_title(&window_title(path));
        }
        Err(e) => show_error(&format!("Cannot open {}:\n{e}", path.display())),
    }
}

/// Handle the About panel's "Check for updates" action. On Windows this runs
/// the on-demand update check (reports up-to-date / newer available / couldn't
/// check); elsewhere the update mechanism isn't built, so just open the
/// releases page in the browser.
fn check_for_updates() {
    #[cfg(windows)]
    update::check_now();
    #[cfg(not(windows))]
    open_external("https://github.com/StruisICT/InLook/releases/latest");
}

/// Open a fixed http(s) URL (our own About-panel links) in the system browser.
fn open_external(url: &str) {
    // Defense in depth: never launch anything but http/https.
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let wide: Vec<u16> = std::ffi::OsStr::new(url)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = std::ffi::OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // Reason: opening a URL in the default browser requires ShellExecuteW.
        // The URL is one of our own compile-time-constant links (About panel),
        // and only ever http/https per the guard above.
        #[allow(unsafe_code)]
        unsafe {
            ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Decide whether the WebView may navigate to `url`, and intercept the
/// attachment action links the renderer emits. This is the whole click-
/// handling mechanism: no script runs in the page (the CSP forbids it);
/// clicking a link merely *attempts* a navigation, which lands here.
fn handle_navigation(
    url: &str,
    current_bytes: &std::sync::Mutex<Option<std::sync::Arc<Vec<u8>>>>,
    proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
) -> bool {
    if let Some(idx) = url.strip_prefix("inlook://save/") {
        if let Some(bytes) = current_bytes.lock().unwrap().clone() {
            save_attachment(&bytes, idx);
        }
        return false;
    }
    if let Some(idx) = url.strip_prefix("inlook://open/") {
        if let Some(bytes) = current_bytes.lock().unwrap().clone() {
            open_nested_message(&bytes, idx);
        }
        return false;
    }
    // Welcome-screen / app-bar "Open" and drop zone → open the file picker.
    if url == "inlook://browse" || url == "inlook://browse/" {
        let _ = proxy.send_event(UserEvent::Browse);
        return false;
    }
    // About → "Check for updates": an explicit, on-demand check.
    if url == "inlook://check-update" || url == "inlook://check-update/" {
        check_for_updates();
        return false;
    }
    // Our own About-panel links (Buy Me a Coffee / GitHub / site) → system
    // browser. These are the app's own fixed URLs; the email body can't reach
    // here (it's in a sandboxed iframe that can't navigate the top frame).
    if (url.starts_with("https://") || url.starts_with("http://"))
        && !url.starts_with("http://inlookview.")
    {
        open_external(url);
        return false;
    }
    // Allow the WebView's own document (served from our custom protocol, incl.
    // `#about` fragment navigation) and its inline sub-frames. Everything else —
    // any link an email might smuggle into scope — is blocked.
    url.starts_with("http://inlookview.") // Windows/Android custom-protocol host
        || url.starts_with("inlookview://") // custom-protocol host elsewhere
        || url.starts_with("about:") // about:blank / about:srcdoc (the body iframe)
        || url.starts_with("data:")
        || url == "null"
        || url.is_empty()
}

/// Save-As for a clicked attachment. Always a dialog, never auto-open —
/// attachments from untrusted mail must not reach ShellExecute paths.
fn save_attachment(bytes: &[u8], idx: &str) {
    use inlook::extract::{extract_attachment, Extracted};
    let extracted = idx.parse().ok().and_then(|i| extract_attachment(bytes, i));
    let Some(extracted) = extracted else {
        show_error("Couldn't extract this attachment.");
        return;
    };
    let (name, data) = match extracted {
        Extracted::File { name, data } => (name, data),
        Extracted::Message { name, data, is_msg } => (
            format!("{name}.{}", if is_msg { "msg" } else { "eml" }),
            data,
        ),
    };
    let Some(dest) = rfd::FileDialog::new()
        .set_title(format!("{APP_NAME} — save attachment"))
        .set_file_name(sanitize_filename(&name))
        .save_file()
    else {
        return; // user cancelled
    };
    if let Err(e) = std::fs::write(&dest, &data) {
        show_error(&format!(
            "Couldn't save the attachment to {}:\n{e}",
            dest.display()
        ));
    }
}

/// Open an attached email in a new InLook window: write it to a per-process
/// temp file and spawn another instance of this executable on it.
fn open_nested_message(bytes: &[u8], idx: &str) {
    use inlook::extract::{extract_attachment, Extracted};
    let extracted = idx.parse().ok().and_then(|i| extract_attachment(bytes, i));
    let Some(Extracted::Message { name, data, is_msg }) = extracted else {
        show_error("Couldn't extract this attached message.");
        return;
    };
    let dir = std::env::temp_dir().join(APP_NAME).join("nested");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        show_error(&format!("Couldn't create a temporary folder:\n{e}"));
        return;
    }
    let ext = if is_msg { "msg" } else { "eml" };
    let file = dir.join(format!(
        "{}-{idx}-{}.{ext}",
        std::process::id(),
        sanitize_filename(&name)
    ));
    if let Err(e) = std::fs::write(&file, &data) {
        show_error(&format!("Couldn't write a temporary file:\n{e}"));
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("inlook"));
    if let Err(e) = std::process::Command::new(exe).arg(&file).spawn() {
        show_error(&format!("Couldn't open the attached message:\n{e}"));
    }
}

/// Reduce an attachment name from untrusted mail to something safe to hand a
/// save dialog / temp path: no separators, no control characters, no leading
/// dots, bounded length.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " ._-()[]".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed: String = cleaned.trim_matches([' ', '.']).chars().take(120).collect();
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed
    }
}

/// Read an EML file, refusing anything over [`MAX_FILE_BYTES`]. Avoids
/// allocating gigabytes for a hostile or accidentally-piped huge file.
fn read_eml(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("stat: {e}"))?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "file is {:.1} GiB; the {} GiB limit refused it",
            meta.len() as f64 / (1024.0 * 1024.0 * 1024.0),
            MAX_FILE_BYTES / (1024 * 1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(|e| format!("read: {e}"))
}

fn show_error(msg: &str) {
    rfd::MessageDialog::new()
        .set_title(APP_NAME)
        .set_description(msg)
        .show();
}

/// A per-user, writable directory for WebView2's data folder. Installed builds
/// live in `Program Files`, which standard users can't write to, so WebView2's
/// default (a folder beside `inlook.exe`) fails with access-denied. Store it
/// under `%LOCALAPPDATA%\InLook\WebView2` instead.
#[cfg(windows)]
fn webview_data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let dir = Path::new(&base).join(APP_NAME).join("WebView2");
    // Best-effort: if creation fails, WebView2 surfaces its own error on build.
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Other platforms don't have the Program Files problem; let `WebContext` use
/// its default per-user location.
#[cfg(not(windows))]
fn webview_data_dir() -> Option<PathBuf> {
    None
}

/// Turn a failed WebView build into a clear, actionable message that prompts
/// the user rather than dumping a raw error. On Windows it distinguishes
/// "runtime not installed" (tell them how to install it) from "installed but
/// couldn't start" (usually a data-folder permission problem).
#[cfg(windows)]
fn webview_error_message(e: &wry::Error) -> String {
    if webview2_runtime_installed() {
        format!(
            "InLook couldn't display the email body.\n\n\
             The Microsoft Edge WebView2 Runtime is installed, but it failed to \
             start:\n{e}\n\n\
             This is usually a permissions problem with WebView2's data folder. \
             InLook keeps it here:\n    %LOCALAPPDATA%\\{APP_NAME}\\WebView2\n\n\
             Make sure that folder exists and is writable, then reopen the email."
        )
    } else {
        format!(
            "InLook can't display the email body because the Microsoft Edge \
             WebView2 Runtime isn't installed.\n\n\
             Install it (free, from Microsoft) and then reopen this email:\n\
             https://developer.microsoft.com/microsoft-edge/webview2/\n\n\
             Technical detail: {e}"
        )
    }
}

/// Non-Windows: WebKitGTK/WKWebView back the renderer instead of WebView2.
#[cfg(not(windows))]
fn webview_error_message(e: &wry::Error) -> String {
    format!(
        "InLook couldn't display the email body:\n{e}\n\n\
         On Linux this usually means the WebKitGTK runtime (libwebkit2gtk-4.1) \
         is missing; install it with your package manager and try again."
    )
}

/// Whether the Microsoft Edge WebView2 Runtime looks installed. The Evergreen
/// runtime registers a product version (`pv`) under EdgeUpdate in either the
/// per-machine or per-user hive; a present, non-zero value means it's there.
#[cfg(windows)]
fn webview2_runtime_installed() -> bool {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
    // Evergreen Runtime app GUID. A 64-bit process sees the per-machine entry
    // under WOW6432Node; the second path covers per-user / non-WOW layouts.
    const KEYS: [&str; 2] = [
        "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    ];
    for hive in [LOCAL_MACHINE, CURRENT_USER] {
        for key in KEYS {
            if let Ok(k) = hive.open(key) {
                if let Ok(pv) = k.get_string("pv") {
                    if !pv.is_empty() && pv != "0.0.0.0" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// On Windows GUI subsystem builds, our process has no attached console, so
/// `println!` goes nowhere when invoked from `cmd` / PowerShell. Attaching to
/// the parent process's console makes CLI subcommands print like a normal
/// console app while the GUI viewer path still works.
#[cfg(windows)]
fn attach_parent_console() {
    // Reason: the only way to attach to the parent's console is via a Win32
    // call. The function ignores the result — failure (e.g. no parent console
    // when launched from Explorer) is fine; we just have nowhere to print.
    #[allow(unsafe_code)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
