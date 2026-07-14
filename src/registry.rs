use std::env;
use std::path::Path;
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

const PROGID: &str = "StruisICT.InLook";
const DESCRIPTION: &str = "EML Email Message";
const FRIENDLY: &str = "InLook";

/// Where we keep InLook's own per-user settings (e.g. the "don't ask again"
/// flag for the default-handler prompt).
const SETTINGS_KEY: &str = "Software\\StruisICT\\InLook";

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

    // Notify the shell so the icon and association refresh without reboot.
    notify_shell_assoc_changed();

    Ok(())
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

    notify_shell_assoc_changed();
    Ok(())
}

/// Register this executable as an *available* handler for `.eml` in the current
/// user's hive (`HKCU\Software\Classes`). Needs no elevation. Unlike
/// [`register`], this does not claim the default association — on Windows 10/11
/// only the user can set the default (the per-user `UserChoice` is hash
/// protected). It just makes InLook show up as a proper choice, with its icon
/// and friendly name, in the "Open with" dialog.
pub fn register_per_user() -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_str = exe.to_string_lossy().into_owned();

    let classes = CURRENT_USER
        .create("Software\\Classes")
        .map_err(|e| format!("open HKCU Software\\Classes: {e}"))?;

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

    let ext = classes
        .create(".eml")
        .map_err(|e| format!("create .eml: {e}"))?;
    let owpids = ext
        .create("OpenWithProgids")
        .map_err(|e| format!("create OpenWithProgids: {e}"))?;
    owpids
        .set_string(PROGID, "")
        .map_err(|e| format!("set OpenWithProgids entry: {e}"))?;

    notify_shell_assoc_changed();
    Ok(())
}

/// Whether InLook is already the user's default handler for `.eml`, according to
/// the shell's hash-protected `UserChoice`.
pub fn is_default_eml_handler() -> bool {
    CURRENT_USER
        .open("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.eml\\UserChoice")
        .and_then(|k| k.get_string("ProgId"))
        .map(|p| p == PROGID)
        .unwrap_or(false)
}

/// Whether the user has told us to stop offering to set InLook as the default.
pub fn default_prompt_suppressed() -> bool {
    CURRENT_USER
        .open(SETTINGS_KEY)
        .and_then(|k| k.get_string("SkipDefaultPrompt"))
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Remember that the user doesn't want to be asked again. Best-effort.
pub fn suppress_default_prompt() {
    if let Ok(k) = CURRENT_USER.create(SETTINGS_KEY) {
        let _ = k.set_string("SkipDefaultPrompt", "1");
    }
}

/// Open the Windows "Open with" chooser for `file` with the "Always use this
/// app" option enabled, letting the user set InLook as the default through the
/// OS's own sanctioned path. We omit `OAIF_EXEC` on purpose so choosing an app
/// here doesn't launch a second viewer window.
pub fn open_with_dialog(file: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHOpenWithDialog, OAIF_ALLOW_REGISTRATION, OAIF_REGISTER_EXT, OPENASINFO,
    };

    let wide: Vec<u16> = file
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let info = OPENASINFO {
        pcszFile: PCWSTR(wide.as_ptr()),
        pcszClass: PCWSTR::null(),
        oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT,
    };

    // Reason: SHOpenWithDialog is the documented way to show the shell's
    // "Open with" chooser. It takes a raw pointer to OPENASINFO and an optional
    // parent HWND (none — the dialog stands alone). `wide` must outlive the call.
    #[allow(unsafe_code)]
    unsafe {
        SHOpenWithDialog(None, &info).map_err(|e| format!("SHOpenWithDialog: {e}"))
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
