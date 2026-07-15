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
                println!("{APP_NAME} is registered as an .eml handler.");
                println!("Opening Windows Settings on the {APP_NAME} page — click \"Set default\"");
                println!("(Windows 11) or pick {APP_NAME} for .eml (Windows 10) to finish.");
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

    let _webview = match WebViewBuilder::new(&window).with_html(html).build() {
        Ok(v) => v,
        Err(e) => {
            show_error(&format!(
                "Failed to create WebView2 surface: {e}\n\nMake sure the Microsoft Edge WebView2 Runtime is installed."
            ));
            return ExitCode::FAILURE;
        }
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
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
