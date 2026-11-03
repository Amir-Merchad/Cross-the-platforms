//! Raw Input mouse deltas + low-level hooks that swallow local input while SUPPRESS is set.
//! Hooks never block: everything goes to the engine thread over a channel. Moves are always
//! reported (with the cursor position, for edge entry); buttons, wheel and keys only while remote.

use std::{
    sync::{atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering}, mpsc::Sender, Mutex, OnceLock},
    time::{Duration, Instant},
};

use bridge_core::router::Input;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Input::{
                GetRawInputData, HRAWINPUT, KeyboardAndMouse::VK_ESCAPE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
                RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE, RegisterRawInputDevices,
            },
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

use crate::keys::{KeyGate, Verdict};

/// While true, local input is swallowed and forwarded to the active device.
pub static SUPPRESS: AtomicBool = AtomicBool::new(false);
static KB_HOOK: AtomicIsize = AtomicIsize::new(0);
static MS_HOOK: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static TX: OnceLock<Mutex<Option<Sender<Input>>>> = OnceLock::new();

/// Ctrl+Alt+1…n only exist for placed devices; the engine keeps this in sync with the layout.
pub fn set_hotkey_count(n: u8) { KB_GATE.lock().unwrap().set_hotkey_count(n); }

/// Removes the hooks and gives input back to Windows. Safe to call more than once.
pub fn unhook() {
    if KB_HOOK.load(Ordering::SeqCst) != 0 { crate::diag::log("hooks removed"); }
    SUPPRESS.store(false, Ordering::SeqCst);
    for h in [&KB_HOOK, &MS_HOOK] {
        let v = h.swap(0, Ordering::SeqCst);
        if v != 0 { unsafe { let _ = UnhookWindowsHookEx(HHOOK(v as _)); } }
    }
}

/// Diagnostics, logged at most once a second from WM_INPUT: calls, worst lag (event time → callback)
/// and worst callback time. Raw keyboard input is the witness: keys Windows saw, hooked or not.
struct HookStats { calls: AtomicU32, lag_ms: AtomicU32, busy_us: AtomicU32 }
static KB_STATS: HookStats = HookStats { calls: AtomicU32::new(0), lag_ms: AtomicU32::new(0), busy_us: AtomicU32::new(0) };
static MS_STATS: HookStats = HookStats { calls: AtomicU32::new(0), lag_ms: AtomicU32::new(0), busy_us: AtomicU32::new(0) };
static RAW_COUNT: AtomicU32 = AtomicU32::new(0);
static RAW_KB: AtomicU32 = AtomicU32::new(0);
static mut LAST_STATS: Option<Instant> = None;

fn record(s: &HookStats, event_time: u32, start: Instant) {
    s.calls.fetch_add(1, Ordering::Relaxed);
    s.lag_ms.fetch_max(unsafe { windows::Win32::System::SystemInformation::GetTickCount() }.wrapping_sub(event_time).min(60_000), Ordering::Relaxed);
    s.busy_us.fetch_max(start.elapsed().as_micros() as u32, Ordering::Relaxed);
}

fn log_stats() {
    let take = |s: &HookStats| (s.calls.swap(0, Ordering::Relaxed), s.lag_ms.swap(0, Ordering::Relaxed), s.busy_us.swap(0, Ordering::Relaxed));
    let (kb, ms, raw, raw_kb) = (take(&KB_STATS), take(&MS_STATS), RAW_COUNT.swap(0, Ordering::Relaxed), RAW_KB.swap(0, Ordering::Relaxed));
    let suppress = SUPPRESS.load(Ordering::SeqCst);
    let skipped = raw_kb > 0 && kb.0 == 0;
    if suppress || skipped || kb.1 > 100 || ms.1 > 100 || kb.2 > 50_000 || ms.2 > 50_000 {
        crate::diag::log(format_args!(
            "stats suppress={suppress} kb calls={} lag={}ms busy={}us raw_kb={raw_kb}{} | ms calls={} lag={}ms busy={}us | raw={raw} | fg {}",
            kb.0, kb.1, kb.2, if skipped { " HOOK SKIPPED" } else { "" }, ms.0, ms.1, ms.2, crate::diag::foreground()
        ));
    }
}

fn send(i: Input) {
    if let Some(tx) = TX.get().and_then(|m| m.lock().ok()).and_then(|g| g.clone()) { let _ = tx.send(i); }
}

pub struct CaptureHandle { thread: Option<std::thread::JoinHandle<()>> }

impl CaptureHandle {
    /// Ends the message loop, removes the hooks and joins the capture thread.
    pub fn stop(&mut self) {
        let id = THREAD.swap(0, Ordering::SeqCst);
        if id != 0 { unsafe { let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)); } }
        if let Some(t) = self.thread.take() { let _ = t.join(); }
        unhook();
    }
}

impl Drop for CaptureHandle { fn drop(&mut self) { self.stop() } }

/// Starts capture on its own thread (hooks need a message loop on the thread that installs them).
pub fn start(tx: Sender<Input>) -> CaptureHandle {
    *TX.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(tx);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        crate::diag::log(format_args!("PANIC on thread {:?}: {info}", std::thread::current().name()));
        unhook();
        prev(info)
    }));
    let thread = std::thread::spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let hinst = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: w!("ctp-capture"), ..Default::default() };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(Default::default(), w!("ctp-capture"), w!(""), Default::default(), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst.into()), None).unwrap();
        let dev = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        let kb = RAWINPUTDEVICE { usUsage: 0x06, ..dev };
        RegisterRawInputDevices(&[dev, kb], size_of::<RAWINPUTDEVICE>() as u32).unwrap();
        KB_HOOK.store(SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), Some(hinst.into()), 0).unwrap().0 as isize, Ordering::SeqCst);
        MS_HOOK.store(SetWindowsHookExW(WH_MOUSE_LL, Some(ms_hook), Some(hinst.into()), 0).unwrap().0 as isize, Ordering::SeqCst);
        crate::diag::log("hooks installed");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        unhook();
    });
    CaptureHandle { thread: Some(thread) }
}

const RI_MOUSE_LEFT_BUTTON_DOWN: u32 = 0x0001;
const RI_MOUSE_LEFT_BUTTON_UP: u32 = 0x0002;
const RI_MOUSE_RIGHT_BUTTON_DOWN: u32 = 0x0004;
const RI_MOUSE_RIGHT_BUTTON_UP: u32 = 0x0008;
const RI_MOUSE_MIDDLE_BUTTON_DOWN: u32 = 0x0010;
const RI_MOUSE_MIDDLE_BUTTON_UP: u32 = 0x0020;
const RI_MOUSE_WHEEL: u32 = 0x0400;
const RI_MOUSE_HWHEEL: u32 = 0x0800;
const MOUSE_MOVE_ABSOLUTE: u16 = 0x0001;
const WHEEL_DELTA: i32 = 120;

/// Hi-res wheels and precision touchpads send sub-notch deltas; keep the remainder per axis.
static mut WHEEL_ACC: (i32, i32) = (0, 0);

#[allow(static_mut_refs)]
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg != WM_INPUT { return unsafe { DefWindowProcW(hwnd, msg, wp, lp) } }
    unsafe {
        if LAST_STATS.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) { LAST_STATS = Some(Instant::now()); log_stats(); }
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        if GetRawInputData(HRAWINPUT(lp.0 as _), RID_INPUT, Some(&mut raw as *mut _ as _), &mut size, size_of::<RAWINPUTHEADER>() as u32) == u32::MAX {
            return LRESULT(0);
        }
        if raw.header.dwType == RIM_TYPEKEYBOARD.0 { RAW_KB.fetch_add(1, Ordering::Relaxed); return LRESULT(0) }
        if raw.header.dwType != RIM_TYPEMOUSE.0 { return LRESULT(0) }
        RAW_COUNT.fetch_add(1, Ordering::Relaxed);
        let m = raw.data.mouse;
        // Pens, touchscreens and remote-desktop sessions report absolute positions: not deltas.
        if m.usFlags.0 & MOUSE_MOVE_ABSOLUTE != 0 { return LRESULT(0) }
        let (dx, dy) = (m.lLastX, m.lLastY);
        if dx != 0 || dy != 0 {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            send(Input::Move { dx, dy, cursor: (p.x, p.y) });
        }
        if !SUPPRESS.load(Ordering::SeqCst) { WHEEL_ACC = (0, 0); return LRESULT(0) }
        let flags = m.Anonymous.Anonymous.usButtonFlags as u32;
        for (i, (down, up)) in [(RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP), (RI_MOUSE_RIGHT_BUTTON_DOWN, RI_MOUSE_RIGHT_BUTTON_UP), (RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP)].iter().enumerate() {
            if flags & *down != 0 { send(Input::Button { mask: 1 << i, down: true }); }
            if flags & *up != 0 { send(Input::Button { mask: 1 << i, down: false }); }
        }
        let data = m.Anonymous.Anonymous.usButtonData as i16 as i32;
        if flags & RI_MOUSE_WHEEL != 0 { WHEEL_ACC.0 += data }
        if flags & RI_MOUSE_HWHEEL != 0 { WHEEL_ACC.1 += data }
        let (v, h) = (WHEEL_ACC.0 / WHEEL_DELTA, WHEEL_ACC.1 / WHEEL_DELTA);
        if v != 0 || h != 0 {
            WHEEL_ACC.0 -= v * WHEEL_DELTA;
            WHEEL_ACC.1 -= h * WHEEL_DELTA;
            send(Input::Wheel { v: v.clamp(-127, 127) as i8, h: h.clamp(-127, 127) as i8 });
        }
    }
    LRESULT(0)
}

static mut ESC_TIMES: [Option<Instant>; 3] = [None; 3];
static KB_GATE: Mutex<KeyGate> = Mutex::new(KeyGate::new());
static MS_GATE: Mutex<KeyGate> = Mutex::new(KeyGate::new());

/// Give input back to Windows first, then tell the engine (which may be stuck in a BLE notify).
fn back_to_windows(event: Input) -> LRESULT {
    SUPPRESS.store(false, Ordering::SeqCst);
    send(event);
    LRESULT(1)
}

unsafe extern "system" fn kb_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let start = Instant::now();
    let r = unsafe { kb_hook_inner(code, wp, lp) };
    if code == HC_ACTION as i32 { record(&KB_STATS, unsafe { (*(lp.0 as *const KBDLLHOOKSTRUCT)).time }, start) }
    r
}

unsafe extern "system" fn ms_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let start = Instant::now();
    let r = unsafe { ms_hook_inner(code, wp, lp) };
    if code == HC_ACTION as i32 { record(&MS_STATS, unsafe { (*(lp.0 as *const MSLLHOOKSTRUCT)).time }, start) }
    r
}

#[allow(static_mut_refs)]
unsafe fn kb_hook_inner(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let vk = k.vkCode as u16;
            let suppress = SUPPRESS.load(Ordering::SeqCst);
            let verdict = KB_GATE.lock().unwrap().on_key(vk, down, suppress);
            if down { crate::diag::log(format_args!("kb hook: down suppress={suppress} -> {verdict:?}")); }
            match verdict {
                Verdict::PassToWindows => {}
                Verdict::Hotkey(0) => return back_to_windows(Input::Hotkey(0)),
                Verdict::Hotkey(n) => { send(Input::Hotkey(n)); return LRESULT(1) }
                Verdict::Drop => return LRESULT(1),
                Verdict::Remote => {
                    if down && vk == VK_ESCAPE.0 {
                        ESC_TIMES.rotate_left(1);
                        ESC_TIMES[2] = Some(Instant::now());
                        if ESC_TIMES[0].is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                            ESC_TIMES = [None; 3];
                            return back_to_windows(Input::Escape);
                        }
                    }
                    send(Input::Key { vk, down });
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

unsafe fn ms_hook_inner(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let suppress = SUPPRESS.load(Ordering::SeqCst);
            let button = match wp.0 as u32 {
                WM_LBUTTONDOWN => Some((1, true)), WM_LBUTTONUP => Some((1, false)),
                WM_RBUTTONDOWN => Some((2, true)), WM_RBUTTONUP => Some((2, false)),
                WM_MBUTTONDOWN => Some((4, true)), WM_MBUTTONUP => Some((4, false)),
                _ => None,
            };
            // Buttons pressed on Windows before crossing must be released on Windows.
            let swallow = match button {
                Some((vk, down)) => MS_GATE.lock().unwrap().on_key(vk, down, suppress) != Verdict::PassToWindows,
                None => suppress,
            };
            if swallow { return LRESULT(1) }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}
