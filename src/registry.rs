use std::env;
use windows_registry::LOCAL_MACHINE;

const PROGID: &str = "StruisICT.InLook";
const DESCRIPTION: &str = "EML Email Message";
const FRIENDLY: &str = "InLook";
/// Name under `HKLM\SOFTWARE\RegisteredApplications` — this is the app name
/// Windows Settings shows on the Default Apps page, and the value the
/// `ms-settings:defaultapps?registeredAppMachine=` deep link matches on.
const REGISTERED_APP: &str = "InLook";
const CAPABILITIES_PATH: &str = "Software\\StruisICT\\InLook\\Capabilities";

/// Register this executable as the handler for .eml files in HKLM.
/// Requires the process to run elevated.
pub fn register() -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_str = exe.to_string_lossy().into_owned();

    let classes = LOCAL_MACHINE
        .create("Software\\Classes")
        .map_err(|e| format!("open Software\\Classes: {e}"))?;

    // ProgID
    let progid = classes
        .create(PROGID)
        .map_err(|e| format!("create ProgID: {e}"))?;
    progid
        .set_string("", DESCRIPTION)
        .map_err(|e| format!("set ProgID default: {e}"))?;
    progid
        .set_string("FriendlyTypeName", FRIENDLY)
        .map_err(|e| format!("set FriendlyTypeName: {e}"))?;

    let icon = progid
        .create("DefaultIcon")
        .map_err(|e| format!("create DefaultIcon: {e}"))?;
    icon.set_string("", &format!("\"{exe_str}\",0"))
        .map_err(|e| format!("set DefaultIcon: {e}"))?;

    let cmd = progid
        .create("shell\\open\\command")
        .map_err(|e| format!("create shell\\open\\command: {e}"))?;
    cmd.set_string("", &format!("\"{exe_str}\" \"%1\""))
        .map_err(|e| format!("set open command: {e}"))?;

    // .eml extension → ProgID
    let ext = classes
        .create(".eml")
        .map_err(|e| format!("create .eml: {e}"))?;
    ext.set_string("", PROGID)
        .map_err(|e| format!("set .eml default: {e}"))?;
    ext.set_string("Content Type", "message/rfc822")
        .map_err(|e| format!("set content type: {e}"))?;

    let owpids = ext
        .create("OpenWithProgids")
        .map_err(|e| format!("create OpenWithProgids: {e}"))?;
    owpids
        .set_string(PROGID, "")
        .map_err(|e| format!("set OpenWithProgids entry: {e}"))?;

    // Default Programs registration. Without this, InLook never appears as an
    // *application* on the Settings ▸ Default apps page (only as a bare ProgID
    // in "Open with"), and the ms-settings deep link below has nothing to
    // land on. With it, Windows 11 shows InLook with its declared file types
    // and a one-click "Set default" button — the Chrome-style flow.
    let caps = LOCAL_MACHINE
        .create(CAPABILITIES_PATH)
        .map_err(|e| format!("create Capabilities: {e}"))?;
    caps.set_string("ApplicationName", REGISTERED_APP)
        .map_err(|e| format!("set ApplicationName: {e}"))?;
    caps.set_string(
        "ApplicationDescription",
        "Fast, safe .eml email viewer — Free Software from Struis ICT",
    )
    .map_err(|e| format!("set ApplicationDescription: {e}"))?;
    caps.set_string("ApplicationIcon", &format!("\"{exe_str}\",0"))
        .map_err(|e| format!("set ApplicationIcon: {e}"))?;
    let assoc = caps
        .create("FileAssociations")
        .map_err(|e| format!("create FileAssociations: {e}"))?;
    assoc
        .set_string(".eml", PROGID)
        .map_err(|e| format!("set FileAssociations .eml: {e}"))?;
    let regapps = LOCAL_MACHINE
        .create("Software\\RegisteredApplications")
        .map_err(|e| format!("open RegisteredApplications: {e}"))?;
    regapps
        .set_string(REGISTERED_APP, CAPABILITIES_PATH)
        .map_err(|e| format!("set RegisteredApplications entry: {e}"))?;

    // Notify the shell so the icon and association refresh without reboot.
    notify_shell_assoc_changed();

    Ok(())
}

/// Open Windows Settings on InLook's Default Apps page, where the user
/// finishes the job with one click ("Set default" on Windows 11; picking
/// InLook for .eml on Windows 10, where the deep-link parameter is ignored
/// and the general Default Apps page opens instead).
///
/// This indirection exists because Windows does not let a program set the
/// per-user default handler itself: the UserChoice key is protected by a
/// hash, by design. Opening the right Settings page is exactly what browsers
/// like Chrome do for "Make default".
pub fn open_default_apps_settings() {
    use windows::{
        core::w,
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    // Reason: launching a ms-settings: URI requires ShellExecuteW; there is
    // no safe wrapper. Failure (e.g. Settings unavailable on stripped-down
    // SKUs) just means the user follows the printed instructions instead.
    #[allow(unsafe_code)]
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:defaultapps?registeredAppMachine=InLook"),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// Remove our ProgID and our entries from .eml. Leaves any unrelated handlers
/// (e.g. Outlook) intact — only clears .eml's default if it currently points
/// at us.
pub fn unregister() -> Result<(), String> {
    let classes = LOCAL_MACHINE
        .create("Software\\Classes")
        .map_err(|e| format!("open Software\\Classes: {e}"))?;

    if let Ok(ext) = classes.open(".eml") {
        // Only clear the default if it's our ProgID
        if let Ok(current) = ext.get_string("") {
            if current == PROGID {
                let _ = ext.set_string("", "");
            }
        }
        if let Ok(owpids) = ext.open("OpenWithProgids") {
            let _ = owpids.remove_value(PROGID);
        }
    }

    let _ = classes.remove_tree(PROGID);

    // Default Programs registration
    if let Ok(regapps) = LOCAL_MACHINE.create("Software\\RegisteredApplications") {
        let _ = regapps.remove_value(REGISTERED_APP);
    }
    let _ = LOCAL_MACHINE.remove_tree("Software\\StruisICT\\InLook");

    notify_shell_assoc_changed();
    Ok(())
}

/// Tell Explorer to refresh its file-association cache so the new icon and
/// "Open with" handler appear without a logoff. This is a fire-and-forget
/// notification — no failure mode worth handling.
fn notify_shell_assoc_changed() {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    // Reason: SHChangeNotify is the only way to broadcast association
    // changes; the call has no error path to inspect.
    #[allow(unsafe_code)]
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
