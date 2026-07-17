//! Optional, opt-in update check (Windows only).
//!
//! **Off by default.** InLook makes no network call until the user explicitly
//! answers "yes" to a one-time consent prompt (stored in HKCU). This keeps the
//! offline-by-default guarantee intact — see the README privacy policy.
//!
//! When enabled, the check is deliberately minimal and uses **no bundled HTTP
//! or TLS library**: it goes through the operating system's own HTTPS stack
//! (WinHTTP / Schannel), so certificate validation is the OS's and nothing
//! third-party is compiled into the security-critical binary. It performs a
//! single redirect-suppressed GET to the public "latest release" URL, reads
//! the `Location` header to learn the newest tag, compares it to the running
//! version, and shows one dialog per new version. It never downloads or runs
//! anything — it only tells the user to update via winget or the release page.

use crate::registry;
use inlook::version;
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_LOCATION,
};

const HOST: PCWSTR = w!("github.com");
const PATH: PCWSTR = w!("/StruisICT/InLook/releases/latest");
const RELEASES_URL: PCWSTR = w!("https://github.com/StruisICT/InLook/releases/latest");
const HTTPS_PORT: u16 = 443;

/// Run the update flow after the window has painted. `may_prompt` should be
/// false when another first-run dialog (the default-handler offer) is showing
/// this run, so the user never faces two prompts at once — consent is deferred
/// to the next launch. The actual check only runs if already opted in.
pub fn maybe_run(may_prompt: bool) {
    if !registry::update_prompt_answered() {
        if !may_prompt {
            return;
        }
        let enabled = ask_consent();
        registry::set_update_check(enabled);
        if !enabled {
            return;
        }
    }
    if !registry::update_check_enabled() {
        return;
    }

    // Network + dialog off the UI thread; the viewer stays responsive and a
    // slow or failed check never blocks reading the email.
    std::thread::spawn(|| {
        let Some(tag) = fetch_latest_tag() else {
            return;
        };
        let current = env!("CARGO_PKG_VERSION");
        if !version::is_newer(&tag, current) {
            return;
        }
        let normalized = tag.trim_start_matches('v').to_string();
        if registry::last_notified_version().as_deref() == Some(normalized.as_str()) {
            return; // already announced this version
        }
        registry::set_last_notified_version(&normalized);
        notify_update_available(&normalized, current);
    });
}

/// One-time consent dialog. Cancel/close is treated as "no" (stay offline).
fn ask_consent() -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    matches!(
        MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title(APP_NAME)
            .set_description(
                "Check for InLook updates?\n\n\
                 InLook is offline by default. If you choose Yes, it will \
                 occasionally contact github.com (over HTTPS, using Windows' \
                 own secure connection) to see whether a newer version exists. \
                 It never downloads or installs anything automatically, and \
                 sends no information about you or your email.\n\n\
                 You can change this later in the registry value \
                 HKCU\\Software\\StruisICT\\InLook\\UpdateCheckEnabled.",
            )
            .set_buttons(MessageButtons::YesNo)
            .show(),
        MessageDialogResult::Yes
    )
}

/// Tell the user a newer version exists and how to get it. "Yes" opens the
/// releases page in the default browser; nothing is downloaded by InLook.
fn notify_update_available(latest: &str, current: &str) {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let result = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(APP_NAME)
        .set_description(format!(
            "InLook {latest} is available (you have {current}).\n\n\
             To update:\n\
             \u{2022} winget:  winget upgrade StruisICT.InLook\n\
             \u{2022} or download it from the releases page.\n\n\
             Open the releases page now?"
        ))
        .set_buttons(MessageButtons::YesNo)
        .show();
    if matches!(result, MessageDialogResult::Yes) {
        open_releases_page();
    }
}

const APP_NAME: &str = "InLook";

/// Fetch the newest release tag by asking GitHub for the "latest" redirect and
/// reading the `Location` header (e.g. `.../releases/tag/v0.9.0`). Returns
/// `None` on any failure — the feature is best-effort and never surfaces
/// network errors to the user.
fn fetch_latest_tag() -> Option<String> {
    // RAII so every early return closes its WinHTTP handles.
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // Reason: releasing a WinHTTP handle requires the FFI call;
                // there is no safe wrapper and no meaningful error to handle.
                #[allow(unsafe_code)]
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    // Reason: the WinHTTP client API is unsafe FFI. Each handle is wrapped in
    // `Handle` for cleanup; all pointers passed below (wide-string literals via
    // `w!`, and the stack `buf`/`len`) outlive their calls.
    #[allow(unsafe_code)]
    unsafe {
        let session = Handle(WinHttpOpen(
            w!("InLook-update-check"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        let connect = Handle(WinHttpConnect(session.0, HOST, HTTPS_PORT, 0));
        if connect.0.is_null() {
            return None;
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PATH,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return None;
        }

        // Suppress auto-redirect so we can read the 302's Location ourselves.
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_DISABLE_FEATURE,
            Some(&WINHTTP_DISABLE_REDIRECTS.to_le_bytes()),
        )
        .ok()?;

        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;

        // Read the Location header into a fixed buffer (release URLs are short).
        let mut buf = [0u16; 512];
        let mut len = (buf.len() * std::mem::size_of::<u16>()) as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_LOCATION,
            PCWSTR::null(),
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;

        let n = (len as usize) / std::mem::size_of::<u16>();
        let location = String::from_utf16_lossy(&buf[..n]);
        tag_from_location(&location)
    }
}

/// Extract the version tag from a `.../releases/tag/<tag>` URL. Pure so it can
/// be unit-tested without any network.
fn tag_from_location(location: &str) -> Option<String> {
    let tag = location.trim_end_matches('/').rsplit('/').next()?;
    if tag.is_empty() || !tag.starts_with('v') {
        return None;
    }
    Some(tag.to_string())
}

/// Open the releases page in the default browser. Fixed literal URL — no
/// email- or network-derived data reaches the shell.
fn open_releases_page() {
    use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    // Reason: launching a URL requires ShellExecuteW; the URL is a compile-time
    // constant, so there is nothing untrusted in the call.
    #[allow(unsafe_code)]
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            RELEASES_URL,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::tag_from_location;

    #[test]
    fn extracts_tag_from_github_redirect() {
        assert_eq!(
            tag_from_location("https://github.com/StruisICT/InLook/releases/tag/v0.9.0"),
            Some("v0.9.0".to_string())
        );
        assert_eq!(
            tag_from_location("https://github.com/StruisICT/InLook/releases/tag/v1.0.0/"),
            Some("v1.0.0".to_string())
        );
    }

    #[test]
    fn rejects_unexpected_locations() {
        // No tag segment / not a version.
        assert_eq!(tag_from_location("https://github.com/login"), None);
        assert_eq!(tag_from_location(""), None);
        assert_eq!(
            tag_from_location("https://github.com/StruisICT/InLook/releases"),
            None
        );
    }
}
