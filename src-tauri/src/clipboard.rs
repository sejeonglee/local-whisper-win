use std::ptr::null;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
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

struct ClipboardSnapshot {
    data_object: Option<IDataObject>,
    plain_text: Option<String>,
    is_empty: bool,
}

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
    let previous_clipboard = snapshot_clipboard();
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
    let restore_result = restore_clipboard(previous_clipboard);
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

fn snapshot_clipboard() -> ClipboardSnapshot {
    let plain_text = snapshot_plain_text_clipboard().unwrap_or_else(|err| {
        debug_log::append(format!(
            "snapshot_plain_text_clipboard failed: {err}, proceeding with IDataObject snapshot only"
        ));
        None
    });
    let ole_result = unsafe { OleGetClipboard() };

    match ole_result {
        Ok(data_object) => ClipboardSnapshot {
            data_object: Some(data_object),
            plain_text,
            is_empty: false,
        },
        Err(err) => {
            let is_empty = if clipboard_is_empty_error(&err.to_string()) {
                true
            } else {
                debug_log::append(format!("snapshot_clipboard failed to get IDataObject: {err}"));
                false
            };

            ClipboardSnapshot {
                data_object: None,
                plain_text,
                is_empty,
            }
        }
    }
}

fn restore_clipboard(previous: ClipboardSnapshot) -> Result<(), String> {
    if let Some(data) = previous.data_object {
        if let Err(err) = restore_clipboard_from_data_object(&data) {
            debug_log::append(format!(
                "restore_clipboard from IDataObject failed: {err}, attempting text fallback"
            ));
            if let Some(text) = previous.plain_text {
                return set_text_clipboard(&text);
            }
            return Err(err);
        }
        return Ok(());
    }

    if let Some(text) = previous.plain_text {
        return set_text_clipboard(&text);
    }

    if previous.is_empty {
        with_open_clipboard(|| {
            unsafe { EmptyClipboard() }.map_err(|err| format!("Failed to clear clipboard: {err}"))?;
            Ok(())
        })?;
        return Ok(());
    }

    debug_log::append("restore_clipboard skipped: no snapshot could be captured");
    Err("Failed to restore clipboard because the previous clipboard snapshot was unavailable".to_string())
}

fn restore_clipboard_from_data_object(data: &IDataObject) -> Result<(), String> {
    let mut last_error = None;

    for _ in 0..10 {
        match unsafe { OleSetClipboard(data) } {
            Ok(()) => match unsafe { OleFlushClipboard() } {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(format!("Failed to flush restored clipboard: {err}"));
                }
            },
            Err(err) => {
                last_error = Some(format!("Failed to restore clipboard: {err}"));
            }
        }

        thread::sleep(Duration::from_millis(40));
    }

    Err(last_error.unwrap_or_else(|| {
        "Failed to restore clipboard for an unknown reason.".to_string()
    }))
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

fn snapshot_plain_text_clipboard() -> Result<Option<String>, String> {
    with_open_clipboard(|| {
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32) }.is_err() {
            return Ok(None);
        }

        let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) }
            .map_err(|err| format!("Failed to read unicode clipboard data: {err}"))?;
        let handle = HGLOBAL(handle.0);
        let byte_len = unsafe { GlobalSize(handle) } as usize;
        if byte_len == 0 {
            return Ok(None);
        }

        let lock = unsafe { GlobalLock(handle) as *const u16 };
        if lock.is_null() {
            return Err("Failed to lock clipboard unicode text data.".to_string());
        }

        let len = byte_len / std::mem::size_of::<u16>();
        let raw = unsafe { std::slice::from_raw_parts(lock, len) };
        let terminator = raw.iter().position(|&ch| ch == 0).unwrap_or(raw.len());
        let text = String::from_utf16_lossy(&raw[..terminator]);
        let _ = unsafe { GlobalUnlock(handle) };

        Ok(Some(text))
    })
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
