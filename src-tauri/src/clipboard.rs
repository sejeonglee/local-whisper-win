use std::ptr::null;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
    CF_UNICODETEXT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsWindow, IsWindowVisible, SetForegroundWindow,
};

use crate::debug_log;

static PASTE_TARGET: OnceLock<Mutex<Option<isize>>> = OnceLock::new();

pub fn capture_paste_target() {
    let target = unsafe { GetForegroundWindow() };
    if target.is_invalid() {
        debug_log::append("capture_paste_target skipped: no foreground window");
        return;
    }

    if let Ok(mut guard) = paste_target().lock() {
        *guard = Some(target.0 as isize);
    }
    debug_log::append(format!("captured paste target hwnd={:?}", target.0));
}

pub fn clear_paste_target() {
    if let Ok(mut guard) = paste_target().lock() {
        *guard = None;
    }
}

pub fn paste_transcription(text: &str) -> Result<(), String> {
    let _ole = OleGuard::acquire()?;
    let previous_clipboard = snapshot_clipboard()?;
    let target = load_paste_target();
    debug_log::append(format!(
        "paste_transcription start chars={} target={}",
        text.chars().count(),
        target.map(|hwnd| hwnd.0 as isize).unwrap_or_default()
    ));

    set_text_clipboard(text)?;

    let paste_result = restore_focus(target).and_then(|_| send_ctrl_v());
    if paste_result.is_ok() {
        thread::sleep(Duration::from_millis(120));
    }
    let restore_result = restore_clipboard(previous_clipboard.as_ref());
    clear_paste_target();

    match (paste_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), Ok(())) => Err(err),
        (Ok(()), Err(err)) => Err(err),
        (Err(paste_err), Err(restore_err)) => Err(format!(
            "{paste_err} Clipboard restoration also failed: {restore_err}"
        )),
    }
}

fn paste_target() -> &'static Mutex<Option<isize>> {
    PASTE_TARGET.get_or_init(|| Mutex::new(None))
}

fn load_paste_target() -> Option<HWND> {
    paste_target()
        .lock()
        .ok()
        .and_then(|guard| guard.map(|raw| HWND(raw as *mut core::ffi::c_void)))
}

fn snapshot_clipboard() -> Result<Option<IDataObject>, String> {
    unsafe {
        OleGetClipboard().map(Some).or_else(|err| {
            if clipboard_is_empty_error(&err.to_string()) {
                Ok(None)
            } else {
                Err(format!("Failed to snapshot clipboard: {err}"))
            }
        })
    }
}

fn restore_clipboard(previous: Option<&IDataObject>) -> Result<(), String> {
    unsafe {
        match previous {
            Some(data) => {
                let mut last_error = None;

                for _ in 0..10 {
                    match OleSetClipboard(data) {
                        Ok(()) => match OleFlushClipboard() {
                            Ok(()) => return Ok(()),
                            Err(err) => {
                                last_error =
                                    Some(format!("Failed to flush restored clipboard: {err}"));
                            }
                        },
                        Err(err) => {
                            last_error = Some(format!("Failed to restore clipboard: {err}"));
                        }
                    }

                    thread::sleep(Duration::from_millis(40));
                }

                return Err(last_error.unwrap_or_else(|| {
                    "Failed to restore clipboard for an unknown reason.".to_string()
                }));
            }
            None => {
                with_open_clipboard(|| {
                    EmptyClipboard().map_err(|err| format!("Failed to clear clipboard: {err}"))?;
                    Ok(())
                })?;
            }
        }
    }

    Ok(())
}

fn set_text_clipboard(text: &str) -> Result<(), String> {
    let utf16 = encode_utf16_nul(text);
    debug_log::append(format!("set_text_clipboard {} chars", text.chars().count()));

    unsafe {
        with_open_clipboard(|| {
            EmptyClipboard().map_err(|err| format!("Failed to empty clipboard: {err}"))?;

            let byte_len = utf16.len() * std::mem::size_of::<u16>();
            let memory = GlobalAlloc(GMEM_MOVEABLE, byte_len)
                .map_err(|err| format!("Failed to allocate clipboard memory: {err}"))?;
            let lock = GlobalLock(memory) as *mut u16;
            if lock.is_null() {
                return Err("Failed to lock clipboard memory.".to_string());
            }

            std::ptr::copy_nonoverlapping(utf16.as_ptr(), lock, utf16.len());
            let _ = GlobalUnlock(memory);

            let result = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)))
                .map_err(|err| format!("Failed to write text clipboard data: {err}"));

            if result.is_err() {
                let mut owned_memory = memory;
                windows::core::Free::free(&mut owned_memory);
            }

            result.map(|_| ())
        })?;
    }

    Ok(())
}

fn restore_focus(target: Option<HWND>) -> Result<(), String> {
    let Some(target) = target else {
        debug_log::append("restore_focus skipped: no target");
        return Ok(());
    };

    unsafe {
        if !IsWindow(Some(target)).as_bool() || !IsWindowVisible(target).as_bool() {
            debug_log::append(format!(
                "restore_focus skipped: target hwnd={} invalid",
                target.0 as isize
            ));
            return Ok(());
        }

        let current = GetForegroundWindow();
        if current != target {
            SetForegroundWindow(target)
                .ok()
                .map_err(|err| format!("Failed to restore target window focus: {err}"))?;
            thread::sleep(Duration::from_millis(60));
        }
        debug_log::append(format!(
            "restore_focus complete target hwnd={}",
            target.0 as isize
        ));
    }

    Ok(())
}

fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        keyboard_input(VK_CONTROL, Default::default()),
        keyboard_input(VK_V, Default::default()),
        keyboard_input(VK_V, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(format!(
            "Failed to send paste keystroke sequence. Sent {sent} of {} events.",
            inputs.len()
        ));
    }
    debug_log::append("send_ctrl_v dispatched");

    Ok(())
}

fn keyboard_input(
    key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn with_open_clipboard<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let mut last_error = None;

    for _ in 0..10 {
        let opened = unsafe { OpenClipboard(None) };
        match opened {
            Ok(()) => {
                let result = operation();
                let _ = unsafe { CloseClipboard() };
                return result;
            }
            Err(err) => {
                last_error = Some(err.to_string());
                thread::sleep(Duration::from_millis(30));
            }
        }
    }

    Err(format!(
        "Failed to open clipboard after retries: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn encode_utf16_nul(text: &str) -> Vec<u16> {
    let mut encoded: Vec<u16> = text.encode_utf16().collect();
    encoded.push(0);
    encoded
}

fn clipboard_is_empty_error(message: &str) -> bool {
    message.contains("CLIPBRD_E_BAD_DATA") || message.contains("0x800401D3")
}

struct OleGuard;

impl OleGuard {
    fn acquire() -> Result<Self, String> {
        unsafe {
            OleInitialize(Some(null()))
                .map(|_| Self)
                .map_err(|err| format!("Failed to initialize OLE clipboard access: {err}"))
        }
    }
}

impl Drop for OleGuard {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_utf16_nul;

    #[test]
    fn utf16_encoding_appends_trailing_nul() {
        let encoded = encode_utf16_nul("abc");
        assert_eq!(encoded, vec![97, 98, 99, 0]);
    }

    #[test]
    fn utf16_encoding_handles_korean_text() {
        let encoded = encode_utf16_nul("안녕");
        assert_eq!(encoded.last().copied(), Some(0));
        assert!(encoded.len() > 2);
    }
}
