use std::env;
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

const PROGID: &str = "StruisICT.InLook";
const PROGID_MSG: &str = "StruisICT.InLook.msg";
const FRIENDLY: &str = "InLook";

/// (ProgID, Explorer type description) for every ProgID InLook owns.
const PROGIDS: [(&str, &str); 2] = [
    (PROGID, "EML Email Message"),
    (PROGID_MSG, "Outlook Email Message"),
];

/// (extension, ProgID, content type) for every file type InLook handles.
/// `.oft` (Outlook template) shares the `.msg` ProgID — same container format.
const ASSOCIATIONS: [(&str, &str, &str); 3] = [
    (".eml", PROGID, "message/rfc822"),
    (".msg", PROGID_MSG, "application/vnd.ms-outlook"),
    (".oft", PROGID_MSG, "application/vnd.ms-outlook"),
];
/// Name under `HKLM\SOFTWARE\RegisteredApplications` — this is the app name
/// Windows Settings shows on the Default Apps page, and the value the
/// `ms-settings:defaultapps?registeredAppMachine=` deep link matches on.
const REGISTERED_APP: &str = "InLook";
const CAPABILITIES_PATH: &str = "Software\\StruisICT\\InLook\\Capabilities";

/// Where we keep InLook's own per-user settings (the opt-in update-check
/// consent and last-notified version).
const SETTINGS_KEY: &str = "Software\\StruisICT\\InLook";

/// Write one ProgID (type description, icon, open command) under `classes`.
/// Shared by the HKLM and HKCU registration paths.
fn write_progid(
    classes: &windows_registry::Key,
    progid: &str,
    description: &str,
    exe_str: &str,
) -> Result<(), String> {
    let key = classes
        .create(progid)
        .map_err(|e| format!("create ProgID {progid}: {e}"))?;
    key.set_string("", description)
        .map_err(|e| format!("set ProgID default: {e}"))?;
    key.set_string("FriendlyTypeName", FRIENDLY)
        .map_err(|e| format!("set FriendlyTypeName: {e}"))?;
    let icon = key
        .create("DefaultIcon")
        .map_err(|e| format!("create DefaultIcon: {e}"))?;
    icon.set_string("", &format!("\"{exe_str}\",0"))
        .map_err(|e| format!("set DefaultIcon: {e}"))?;
    let cmd = key
        .create("shell\\open\\command")
        .map_err(|e| format!("create shell\\open\\command: {e}"))?;
    cmd.set_string("", &format!("\"{exe_str}\" \"%1\""))
        .map_err(|e| format!("set open command: {e}"))?;
    Ok(())
}

/// Register this executable as the handler for .eml/.msg/.oft files in HKLM.
/// Requires the process to run elevated.
pub fn register() -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_str = exe.to_string_lossy().into_owned();

    let classes = LOCAL_MACHINE
        .create("Software\\Classes")
        .map_err(|e| format!("open Software\\Classes: {e}"))?;

    for (progid, description) in PROGIDS {
        write_progid(&classes, progid, description, &exe_str)?;
    }

    // extension → ProgID
    for (ext_name, progid, content_type) in ASSOCIATIONS {
        let ext = classes
            .create(ext_name)
            .map_err(|e| format!("create {ext_name}: {e}"))?;
        ext.set_string("", progid)
            .map_err(|e| format!("set {ext_name} default: {e}"))?;
        ext.set_string("Content Type", content_type)
            .map_err(|e| format!("set {ext_name} content type: {e}"))?;
        let owpids = ext
            .create("OpenWithProgids")
            .map_err(|e| format!("create OpenWithProgids: {e}"))?;
        owpids
            .set_string(progid, "")
            .map_err(|e| format!("set OpenWithProgids entry: {e}"))?;
    }

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
        "Fast, safe viewer for .eml and Outlook .msg email files — Free Software from Struis ICT",
    )
    .map_err(|e| format!("set ApplicationDescription: {e}"))?;
    caps.set_string("ApplicationIcon", &format!("\"{exe_str}\",0"))
        .map_err(|e| format!("set ApplicationIcon: {e}"))?;
    let assoc = caps
        .create("FileAssociations")
        .map_err(|e| format!("create FileAssociations: {e}"))?;
    for (ext_name, progid, _) in ASSOCIATIONS {
        assoc
            .set_string(ext_name, progid)
            .map_err(|e| format!("set FileAssociations {ext_name}: {e}"))?;
    }
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

/// Remove our ProgIDs and our entries from the extensions we claim. Leaves any
/// unrelated handlers (e.g. Outlook) intact — only clears an extension's
/// default if it currently points at us.
pub fn unregister() -> Result<(), String> {
    let classes = LOCAL_MACHINE
        .create("Software\\Classes")
        .map_err(|e| format!("open Software\\Classes: {e}"))?;

    for (ext_name, progid, _) in ASSOCIATIONS {
        if let Ok(ext) = classes.open(ext_name) {
            // Only clear the default if it's our ProgID
            if let Ok(current) = ext.get_string("") {
                if current == progid {
                    let _ = ext.set_string("", "");
                }
            }
            if let Ok(owpids) = ext.open("OpenWithProgids") {
                let _ = owpids.remove_value(progid);
            }
        }
    }

    for (progid, _) in PROGIDS {
        let _ = classes.remove_tree(progid);
    }

    // Default Programs registration
    if let Ok(regapps) = LOCAL_MACHINE.create("Software\\RegisteredApplications") {
        let _ = regapps.remove_value(REGISTERED_APP);
    }
    let _ = LOCAL_MACHINE.remove_tree("Software\\StruisICT\\InLook");

    notify_shell_assoc_changed();
    Ok(())
}

// --- Optional update check (opt-in, off by default) ---
//
// The whole feature is gated on explicit consent stored here. Until the user
// answers the one-time prompt, InLook makes no network call — preserving the
// "offline by default" guarantee. Values live in HKCU (no elevation).

/// Whether we've already asked the user about update checks (once ever).
pub fn update_prompt_answered() -> bool {
    CURRENT_USER
        .open(SETTINGS_KEY)
        .and_then(|k| k.get_string("UpdateCheckPrompted"))
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Record the user's answer to the update-check consent prompt.
pub fn set_update_check(enabled: bool) {
    if let Ok(k) = CURRENT_USER.create(SETTINGS_KEY) {
        let _ = k.set_string("UpdateCheckPrompted", "1");
        let _ = k.set_string("UpdateCheckEnabled", if enabled { "1" } else { "0" });
    }
}

/// Whether the user opted in to update checks.
pub fn update_check_enabled() -> bool {
    CURRENT_USER
        .open(SETTINGS_KEY)
        .and_then(|k| k.get_string("UpdateCheckEnabled"))
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// The version string we last showed an update notice for — so each new
/// release is announced at most once.
pub fn last_notified_version() -> Option<String> {
    CURRENT_USER
        .open(SETTINGS_KEY)
        .ok()
        .and_then(|k| k.get_string("LastNotifiedVersion").ok())
}

/// Remember that we've announced `version`, so we don't nag again for it.
pub fn set_last_notified_version(version: &str) {
    if let Ok(k) = CURRENT_USER.create(SETTINGS_KEY) {
        let _ = k.set_string("LastNotifiedVersion", version);
    }
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
