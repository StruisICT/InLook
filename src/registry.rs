use std::env;
use windows_registry::LOCAL_MACHINE;

const PROGID: &str = "StruisICT.InLook";
const DESCRIPTION: &str = "EML Email Message";
const FRIENDLY: &str = "InLook";

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

fn notify_shell_assoc_changed() {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
