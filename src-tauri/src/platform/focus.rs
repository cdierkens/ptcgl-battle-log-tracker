//! The active application's process id: `NSWorkspace` on macOS, `GetForegroundWindow` on Windows.

use crate::focus::FocusProbe;

pub struct WorkspaceFocusProbe;

#[cfg(target_os = "macos")]
impl FocusProbe for WorkspaceFocusProbe {
    fn active_pid(&self) -> Option<u32> {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSNumber, NSString};

        let pid_key = NSString::from_str("NSApplicationProcessIdentifier");
        // `frontmostApplication` goes stale off the main run loop (this runs on the
        // engine thread); the deprecated `activeApplication` is real-time.
        #[allow(deprecated)]
        let active = NSWorkspace::sharedWorkspace().activeApplication();
        active
            .and_then(|dict| dict.valueForKey(&pid_key))
            .and_then(|pid| pid.downcast::<NSNumber>().ok())
            .map(|pid| pid.intValue() as u32)
    }
}

#[cfg(target_os = "windows")]
mod win {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    pub(super) fn foreground_pid() -> Option<u32> {
        // SAFETY: neither call takes ownership, and `GetWindowThreadProcessId`
        // writes only to the local handed to it.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
            if pid == 0 {
                None
            } else {
                Some(pid)
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl FocusProbe for WorkspaceFocusProbe {
    fn active_pid(&self) -> Option<u32> {
        win::foreground_pid()
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
impl FocusProbe for WorkspaceFocusProbe {
    fn active_pid(&self) -> Option<u32> {
        None
    }
}
