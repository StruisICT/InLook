#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]
#![warn(clippy::all)]

#[cfg(windows)]
mod registry;

use inlook::render;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const APP_NAME: &str = "InLook";
const BRAND: &str = "Free Software from Struis ICT";

/// Refuse to read files larger than this. EML files are normally well under
/// 25 MiB; anything larger is almost certainly malformed input or a DoS
/// attempt against the parser/renderer.
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

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
                println!(".eml files are now associated with {APP_NAME}.");
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
        Some(path) if !path.starts_with("--") => open_viewer(PathBuf::from(path)),
        Some(other) => {
            eprintln!("Unknown option: {other}");
            print_help();
            ExitCode::FAILURE
        }
        None => match rfd::FileDialog::new()
            .set_title(format!("{APP_NAME} — open .eml file"))
            .add_filter("Email message", &["eml"])
            .pick_file()
        {
            Some(p) => open_viewer(p),
            None => ExitCode::SUCCESS,
        },
    }
}

fn print_help() {
    println!("{APP_NAME} {} — {BRAND}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  inlook <file.eml>     Open an EML file in the viewer window");
    println!("  inlook                Open a file picker");
    println!("  inlook register       Associate .eml with this viewer (admin)");
    println!("  inlook unregister     Remove the .eml association (admin)");
    println!("  inlook --version");
}

fn open_viewer(path: PathBuf) -> ExitCode {
    let bytes = match read_eml(&path) {
        Ok(b) => b,
        Err(e) => {
            show_error(&format!("Cannot open {}:\n{e}", path.display()));
            return ExitCode::FAILURE;
        }
    };

    let html = render::render_eml_to_html(&bytes, &path);

    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let title = format!(
        "{} — {APP_NAME}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("EML"),
    );
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
    let _webview = match WebViewBuilder::new(&window)
        .with_web_context(&mut web_context)
        .with_html(html)
        .build()
    {
        Ok(v) => v,
        Err(e) => {
            show_error(&webview_error_message(&e));
            return ExitCode::FAILURE;
        }
    };

    // On Windows, offer once (after the window has painted) to make InLook the
    // default .eml viewer. Done inside the loop so the email is visible behind
    // the prompt rather than a blank window.
    #[cfg(windows)]
    let prompt_path = path.clone();
    #[cfg(windows)]
    let mut default_prompt_pending = true;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            #[cfg(windows)]
            Event::RedrawEventsCleared if default_prompt_pending => {
                default_prompt_pending = false;
                maybe_offer_default(&prompt_path);
            }
            _ => {}
        }
    });
}

/// Offer, at most once and only if InLook isn't already the default, to make it
/// the default `.eml` viewer. Windows 10/11 won't let an app set the default
/// silently (the per-user choice is hash protected), so on "Set as default" we
/// register InLook as a handler and open the OS "Open with" chooser, where the
/// user confirms via "Always use this app".
#[cfg(windows)]
fn maybe_offer_default(file: &Path) {
    if registry::is_default_eml_handler() || registry::default_prompt_suppressed() {
        return;
    }
    // Make InLook a registered choice first (no elevation needed).
    let _ = registry::register_per_user();

    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let result = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(APP_NAME)
        .set_description(
            "Make InLook your default app for .eml email files?\n\n\
             You'll get a Windows dialog where you can pick InLook and tick \
             \"Always use this app\".",
        )
        .set_buttons(MessageButtons::YesNoCancelCustom(
            "Set as default".to_owned(),
            "Not now".to_owned(),
            "Don't ask again".to_owned(),
        ))
        .show();

    match result {
        MessageDialogResult::Custom(s) if s == "Set as default" => {
            if let Err(e) = registry::open_with_dialog(file) {
                show_error(&format!(
                    "Couldn't open the Windows \"Open with\" dialog:\n{e}"
                ));
            }
        }
        MessageDialogResult::Custom(s) if s == "Don't ask again" => {
            registry::suppress_default_prompt();
        }
        // "Not now" or dialog closed: leave it, we'll offer again next time.
        _ => {}
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
            "file is {} bytes; the {} MiB limit refused it",
            meta.len(),
            MAX_FILE_BYTES / (1024 * 1024)
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
