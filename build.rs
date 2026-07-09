// Build script: on Windows, embed the application icon and version
// metadata into the executable so Explorer shows the InLook icon on
// `inlook.exe` and the Details tab lists publisher/version info.
//
// No-op on non-Windows targets.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/inlook.ico");
        res.set("ProductName", "InLook");
        res.set("FileDescription", "InLook — EML viewer");
        res.set("CompanyName", "Struis ICT");
        res.set("LegalCopyright", "Copyright © 2026 Struis ICT");
        res.set("OriginalFilename", "inlook.exe");
        res.compile().expect("failed to compile Windows resources");
    }
}
