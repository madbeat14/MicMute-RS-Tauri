use std::sync::OnceLock;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

// ── Custom thread messages (WM_APP range 0x8000..0xBFFF) ──
/// Tell hook thread to reinstall the LL hook.
const WM_REINSTALL_HOOK: u32 = 0x8001;
/// Tell hook thread to re-sync RegisterHotKey registrations with TARGET_VKS.
const WM_SYNC_HOTKEYS: u32 = 0x8002;
/// Standard Windows WM_HOTKEY message (fired by RegisterHotKey).
const WM_HOTKEY: u32 = 0x0312;
/// MOD_NOREPEAT flag — don't send repeated WM_HOTKEY while key is held.
const MOD_NOREPEAT: u32 = 0x4000;

static HOTKEY_SENDER: OnceLock<Sender<u32>> = OnceLock::new();
static RECORDING_MODE: AtomicBool = AtomicBool::new(false);
static RECORD_SENDER: OnceLock<Sender<u32>> = OnceLock::new();
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);

static TARGET_VKS: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// Whether each target VK is currently held down. Prevents Windows
/// auto-repeat key-downs from firing the action repeatedly.
static KEY_HELD: [AtomicBool; 3] =
    [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];

/// GetTickCount (ms) of the most recent key-down event (including repeats)
/// per target. Lets a missed key-up self-heal via [`STALE_HOLD_MS`].
static KEY_LAST_DOWN_TICK: [AtomicU32; 3] =
    [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

/// Windows' maximum initial auto-repeat delay is 1000 ms and its slowest
/// repeat interval is ~400 ms, so any real hold produces a key-down at least
/// every 1000 ms. A longer gap means the key-up was lost.
const STALE_HOLD_MS: u32 = 1100;

/// Tick count (ms) of the last VK sent by the LL hook callback.
/// Used to deduplicate if both LL hook and RegisterHotKey fire for the same keypress.
static LAST_LL_SEND_TICK: AtomicU32 = AtomicU32::new(0);

/// Decide whether a key event should fire the hotkey action.
/// Returns `true` for the first key-down after a key-up, or when the previous
/// key-down is older than [`STALE_HOLD_MS`] (a missed key-up). A key-up resets
/// the held flag and never fires itself.
fn should_fire(held: &AtomicBool, last_down: &AtomicU32, is_down: bool, now: u32) -> bool {
    if is_down {
        let prev = last_down.swap(now, Ordering::SeqCst);
        let was_held = held.swap(true, Ordering::SeqCst);
        !was_held || now.wrapping_sub(prev) > STALE_HOLD_MS
    } else {
        held.store(false, Ordering::SeqCst);
        false
    }
}

pub struct HotkeyManager {
    receiver: Receiver<u32>,
    record_receiver: Receiver<u32>,
}

impl HotkeyManager {
    pub fn new(vks: Vec<u32>) -> Self {
        let (sender, receiver) = channel();
        let (rec_sender, record_receiver) = channel();

        if HOTKEY_SENDER.set(sender).is_err() {
            tracing::error!("HotkeyManager::new called more than once — HOTKEY_SENDER already set");
        }
        if RECORD_SENDER.set(rec_sender).is_err() {
            tracing::error!("HotkeyManager::new called more than once — RECORD_SENDER already set");
        }

        for (i, &vk) in vks.iter().take(3).enumerate() {
            TARGET_VKS[i].store(vk, Ordering::SeqCst);
        }

        Self {
            receiver,
            record_receiver,
        }
    }

    /// Spawns the dedicated hook thread and installs the global hook.
    /// Also registers hotkeys via RegisterHotKey as a backup for when
    /// the LL hook is silently removed during modal menu loops.
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

            // Primary: low-level keyboard hook
            install_hook();
            // Backup: RegisterHotKey (works during modal menu loops)
            sync_registered_hotkeys();

            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    match msg.message {
                        WM_REINSTALL_HOOK => {
                            install_hook();
                            sync_registered_hotkeys();
                        }
                        WM_SYNC_HOTKEYS => {
                            sync_registered_hotkeys();
                        }
                        WM_HOTKEY => {
                            // RegisterHotKey backup fired — the LL hook didn't catch this key.
                            // Deduplicate: if the LL hook already sent this VK within 100ms, skip.
                            let id = msg.wParam.0;
                            if (1..=3).contains(&id) {
                                let vk = TARGET_VKS[id - 1].load(Ordering::SeqCst);
                                if vk != 0 && !RECORDING_MODE.load(Ordering::SeqCst) {
                                    let now =
                                        windows::Win32::System::SystemInformation::GetTickCount();
                                    let last = LAST_LL_SEND_TICK.load(Ordering::SeqCst);
                                    if now.wrapping_sub(last) > 100
                                        && let Some(sender) = HOTKEY_SENDER.get() {
                                            let _ = sender.send(vk);
                                        }
                                }
                            }
                        }
                        _ => {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
            }
        });
    }

    pub fn set_hotkeys(&self, vks: Vec<u32>) {
        for i in 0..3 {
            let val = if i < vks.len() { vks[i] } else { 0 };
            TARGET_VKS[i].store(val, Ordering::SeqCst);
            // New bindings must not inherit a stale "held" state.
            KEY_HELD[i].store(false, Ordering::SeqCst);
            KEY_LAST_DOWN_TICK[i].store(0, Ordering::SeqCst);
        }
        // Tell hook thread to update RegisterHotKey registrations
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    tid,
                    WM_SYNC_HOTKEYS,
                    WPARAM(0),
                    LPARAM(0),
                )
                .ok();
            }
        }
    }

    /// Ask the hook thread to reinstall the keyboard hook and re-register hotkeys.
    pub fn ensure_hook_active(&self) {
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    tid,
                    WM_REINSTALL_HOOK,
                    WPARAM(0),
                    LPARAM(0),
                )
                .ok();
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

// ── Low-level keyboard hook (primary mechanism) ──

fn install_hook() {
    unsafe {
        let old = HOOK_HANDLE.swap(0, Ordering::SeqCst);
        if old != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(old as *mut _));
        }
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_callback), None, 0) {
            Ok(hook) => {
                HOOK_HANDLE.store(hook.0 as isize, Ordering::SeqCst);
                tracing::debug!("Low-level keyboard hook installed");
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to install low-level keyboard hook");
            }
        }
    }
}

unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let w_param_u32 = w_param.0 as u32;
        let is_down = w_param_u32 == WM_KEYDOWN || w_param_u32 == WM_SYSKEYDOWN;
        let is_up = w_param_u32 == WM_KEYUP || w_param_u32 == WM_SYSKEYUP;

        if is_down || is_up {
            if l_param.0 == 0 {
                return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
            }
            // SAFETY: When n_code >= 0 and the message is WM_KEYDOWN/WM_KEYUP/WM_SYSKEYDOWN/WM_SYSKEYUP,
            // Windows guarantees l_param points to a valid KBDLLHOOKSTRUCT for the duration of the callback.
            // We additionally guard against null above.
            let kbd_struct = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };

            if RECORDING_MODE.load(Ordering::SeqCst) {
                if is_down {
                    if let Some(sender) = RECORD_SENDER.get() {
                        let _ = sender.send(kbd_struct.vkCode);
                        return windows::Win32::Foundation::LRESULT(1);
                    }
                } else if is_up {
                    // Recording swallows the key-up; clear any held target so a
                    // recording session cannot leave a hotkey stuck.
                    for (i, target_atomic) in TARGET_VKS.iter().enumerate() {
                        if target_atomic.load(Ordering::SeqCst) == kbd_struct.vkCode {
                            KEY_HELD[i].store(false, Ordering::SeqCst);
                        }
                    }
                    return windows::Win32::Foundation::LRESULT(1);
                }
            } else {
                let now = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
                for (i, target_atomic) in TARGET_VKS.iter().enumerate() {
                    let target = target_atomic.load(Ordering::SeqCst);
                    if target != 0 && kbd_struct.vkCode == target {
                        let fire = should_fire(
                            &KEY_HELD[i],
                            &KEY_LAST_DOWN_TICK[i],
                            is_down,
                            now,
                        );
                        if is_down {
                            // Keep the RegisterHotKey backup suppressed for the whole
                            // hold — including auto-repeat key-downs, which do not fire.
                            LAST_LL_SEND_TICK.store(now, Ordering::SeqCst);
                            if fire
                                && let Some(sender) = HOTKEY_SENDER.get() {
                                    let _ = sender.send(kbd_struct.vkCode);
                                }
                        }
                        return windows::Win32::Foundation::LRESULT(1);
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

// ── RegisterHotKey backup (works during modal menu loops) ──

/// Unregister all previous hotkeys and re-register current TARGET_VKS.
/// Must be called from the hook thread (RegisterHotKey is thread-affine).
fn sync_registered_hotkeys() {
    unsafe {
        // Unregister old (ids 1..=3)
        for i in 0..TARGET_VKS.len() {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(
                HWND(std::ptr::null_mut()),
                (i + 1) as i32,
            );
        }
        // Register current targets
        let mut registered_count = 0;
        for (i, target_atomic) in TARGET_VKS.iter().enumerate() {
            let vk = target_atomic.load(Ordering::SeqCst);
            if vk != 0 {
                let result = windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                    HWND(std::ptr::null_mut()),
                    (i + 1) as i32,
                    windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS(MOD_NOREPEAT),
                    vk,
                );
                if let Err(e) = result {
                    tracing::debug!(vk = vk, error = ?e, "RegisterHotKey failed (key may be reserved by another app)");
                } else {
                    registered_count += 1;
                }
            }
        }
        tracing::debug!(count = registered_count, "Backup hotkeys synced");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_fire_first_down_only() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(should_fire(&held, &last, true, 1000));
        assert!(!should_fire(&held, &last, true, 1030));
        assert!(!should_fire(&held, &last, true, 1060));
    }

    #[test]
    fn test_should_fire_after_release() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(should_fire(&held, &last, true, 1000));
        assert!(!should_fire(&held, &last, false, 1050));
        assert!(should_fire(&held, &last, true, 2000));
    }

    #[test]
    fn test_repeat_downs_30ms_apart_do_not_fire() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(should_fire(&held, &last, true, 5000));
        assert!(!should_fire(&held, &last, true, 5030));
        assert!(!should_fire(&held, &last, true, 5060));
        assert!(!should_fire(&held, &last, true, 5090));
    }

    #[test]
    fn test_stale_hold_recovers_without_key_up() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(should_fire(&held, &last, true, 1000));
        assert!(should_fire(&held, &last, true, 1000 + STALE_HOLD_MS + 1));

        // Exactly STALE_HOLD_MS is still a normal (non-stale) repeat.
        let held2 = AtomicBool::new(false);
        let last2 = AtomicU32::new(0);
        assert!(should_fire(&held2, &last2, true, 1000));
        assert!(!should_fire(&held2, &last2, true, 1000 + STALE_HOLD_MS));
    }

    #[test]
    fn test_tick_wraparound_repeat_does_not_fire() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(should_fire(&held, &last, true, u32::MAX - 10));
        assert!(!should_fire(&held, &last, true, 20));
    }

    #[test]
    fn test_should_fire_up_without_down_is_noop() {
        let held = AtomicBool::new(false);
        let last = AtomicU32::new(0);
        assert!(!should_fire(&held, &last, false, 0));
        assert!(should_fire(&held, &last, true, 10));
    }
}
