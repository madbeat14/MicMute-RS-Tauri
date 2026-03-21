use std::sync::OnceLock;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, TranslateMessage,
    HHOOK, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

/// Custom message ID used to tell the hook thread to reinstall its hook.
/// WM_APP range (0x8000..0xBFFF) is reserved for private use.
const WM_REINSTALL_HOOK: u32 = 0x8001;

static HOTKEY_SENDER: OnceLock<Sender<u32>> = OnceLock::new();
static RECORDING_MODE: AtomicBool = AtomicBool::new(false);
static RECORD_SENDER: OnceLock<Sender<u32>> = OnceLock::new();
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
// Stores the raw value of the currently installed HHOOK so it can be replaced
static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);

static TARGET_VKS: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

pub struct HotkeyManager {
    receiver: Receiver<u32>,
    record_receiver: Receiver<u32>,
}

impl HotkeyManager {
    pub fn new(vks: Vec<u32>) -> Self {
        let (sender, receiver) = channel();
        let (rec_sender, record_receiver) = channel();

        let _ = HOTKEY_SENDER.set(sender);
        let _ = RECORD_SENDER.set(rec_sender);

        for (i, &vk) in vks.iter().take(3).enumerate() {
            TARGET_VKS[i].store(vk, Ordering::SeqCst);
        }

        Self {
            receiver,
            record_receiver,
        }
    }

    /// Spawns the dedicated hook thread and installs the global hook.
    /// This should be called exactly once.
    pub fn start_hook(&self) {
        if HOOK_THREAD_ID.load(Ordering::SeqCst) != 0 {
            return;
        }
        thread::spawn(|| {
            unsafe {
                let tid = windows::Win32::System::Threading::GetCurrentThreadId();
                HOOK_THREAD_ID.store(tid, Ordering::SeqCst);

                // Elevate thread priority to prevent Windows from dropping the hook during high system load
                let _ = windows::Win32::System::Threading::SetThreadPriority(
                    windows::Win32::System::Threading::GetCurrentThread(),
                    windows::Win32::System::Threading::THREAD_PRIORITY_TIME_CRITICAL,
                );
            }
            install_hook();
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    if msg.message == WM_REINSTALL_HOOK {
                        // Reinstall the hook to recover from silent removal by Windows
                        install_hook();
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
    }

    pub fn set_hotkeys(&self, vks: Vec<u32>) {
        for i in 0..3 {
            let val = if i < vks.len() { vks[i] } else { 0 };
            TARGET_VKS[i].store(val, Ordering::SeqCst);
        }
    }

    /// Ask the hook thread to reinstall the keyboard hook.
    /// Windows can silently remove WH_KEYBOARD_LL hooks when the hook
    /// procedure doesn't respond within the system timeout (e.g., during
    /// a tray context menu modal loop). Calling this periodically ensures
    /// the hook stays active.
    pub fn ensure_hook_active(&self) {
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    tid,
                    WM_REINSTALL_HOOK,
                    WPARAM(0),
                    LPARAM(0),
                ).ok();
            }
        }
    }


    pub fn try_recv(&self) -> Option<u32> {
        self.receiver.try_recv().ok()
    }

    pub fn start_recording(&self) {
        RECORDING_MODE.store(true, Ordering::SeqCst);
    }

    pub fn stop_recording(&self) {
        RECORDING_MODE.store(false, Ordering::SeqCst);
        // Drain any pending recorded key so it doesn't leak into the next session
        while self.record_receiver.try_recv().is_ok() {}
    }

    pub fn try_recv_record(&self) -> Option<u32> {
        if let Ok(vk) = self.record_receiver.try_recv() {
            RECORDING_MODE.store(false, Ordering::SeqCst);
            Some(vk)
        } else {
            None
        }
    }
}

/// Installs (or re-installs) the WH_KEYBOARD_LL hook.
/// Removes any previously installed hook first.
fn install_hook() {
    unsafe {
        // Remove old hook if present
        let old = HOOK_HANDLE.swap(0, Ordering::SeqCst);
        if old != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(old as *mut _));
        }
        // Install new hook
        if let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_callback), None, 0) {
            HOOK_HANDLE.store(hook.0 as isize, Ordering::SeqCst);
        }
    }
}

unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let w_param_u32 = w_param.0 as u32;
        let is_down = w_param_u32 == WM_KEYDOWN || w_param_u32 == WM_SYSKEYDOWN;
        let is_up = w_param_u32 == WM_KEYUP || w_param_u32 == WM_SYSKEYUP;

        if is_down || is_up {
            let kbd_struct = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };

            if RECORDING_MODE.load(Ordering::SeqCst) {
                if is_down {
                    if let Some(sender) = RECORD_SENDER.get() {
                        let _ = sender.send(kbd_struct.vkCode);
                        // Consume the keypress during recording
                        return windows::Win32::Foundation::LRESULT(1);
                    }
                } else if is_up {
                    // Also swallow the UP event during recording to prevent accidental triggers
                    return windows::Win32::Foundation::LRESULT(1);
                }
            } else {
                for target_atomic in &TARGET_VKS {
                    let target = target_atomic.load(Ordering::SeqCst);
                    if target != 0 && kbd_struct.vkCode == target {
                        if is_down {
                            if let Some(sender) = HOTKEY_SENDER.get() {
                                let _ = sender.send(kbd_struct.vkCode);
                            }
                        }
                        // Always swallow both DOWN and UP for matched hotkeys
                        return windows::Win32::Foundation::LRESULT(1);
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}
