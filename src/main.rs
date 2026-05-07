#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod render;
#[cfg(windows)]
mod registry;

use std::path::PathBuf;
use std::process::ExitCode;

const APP_NAME: &str = "InLook";
const BRAND: &str = "Free Software from Struis ICT";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("{} {} — {}", APP_NAME, env!("CARGO_PKG_VERSION"), BRAND);
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        #[cfg(windows)]
        Some("register") => match registry::register() {
            Ok(()) => {
                rfd::MessageDialog::new()
                    .set_title(APP_NAME)
                    .set_description(".eml files are now associated with InLook.")
                    .show();
                ExitCode::SUCCESS
            }
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_title(APP_NAME)
                    .set_description(&format!(
                        "Registration failed: {e}\n\nRun this command from an elevated (Administrator) terminal."
                    ))
                    .show();
                ExitCode::FAILURE
            }
        },
        #[cfg(windows)]
        Some("unregister") => match registry::unregister() {
            Ok(()) => ExitCode::SUCCESS,
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
            .set_title(&format!("{} — open .eml file", APP_NAME))
            .add_filter("Email message", &["eml"])
            .pick_file()
        {
            Some(p) => open_viewer(p),
            None => ExitCode::SUCCESS,
        },
    }
}

fn print_help() {
    println!("{} — {}", APP_NAME, BRAND);
    println!();
    println!("Usage:");
    println!("  inlook <file.eml>     Open an EML file in the viewer window");
    println!("  inlook                Open a file picker");
    println!("  inlook register       Associate .eml with this viewer (admin)");
    println!("  inlook unregister     Remove the .eml association (admin)");
    println!("  inlook --version");
}

fn open_viewer(path: PathBuf) -> ExitCode {
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_title(APP_NAME)
                .set_description(&format!("Cannot read {}:\n{e}", path.display()))
                .show();
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
        "{} — {}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("EML"),
        APP_NAME
    );
    let window = match WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 800.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_title(APP_NAME)
                .set_description(&format!("Failed to create window: {e}"))
                .show();
            return ExitCode::FAILURE;
        }
    };

    let _webview = match WebViewBuilder::new(&window).with_html(html).build() {
        Ok(v) => v,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_title(APP_NAME)
                .set_description(&format!(
                    "Failed to create WebView2 surface: {e}\n\nMake sure the Microsoft Edge WebView2 Runtime is installed."
                ))
                .show();
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
