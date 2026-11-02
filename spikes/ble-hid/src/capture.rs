//! Raw Input mouse deltas + low-level hooks that swallow local input while SUPPRESS is set.
//! Hooks never block: everything goes to the sender thread over a channel.

use std::{
    sync::{atomic::{AtomicBool, AtomicIsize, Ordering}, mpsc::Sender, OnceLock},
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::{
                GetRawInputData, HRAWINPUT, KeyboardAndMouse::VK_ESCAPE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
                RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
            },
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

pub enum Input {
    /// Pointer pushed through the configured Windows edge for the hold time.
    Enter,
    Move(i32, i32),
    Button { mask: u8, down: bool },
    Wheel(i8),
    Key { vk: u16, down: bool },
    /// Ctrl+Alt+Q or Esc ×3 within 1 s.
    Escape,
}

pub static SUPPRESS: AtomicBool = AtomicBool::new(false);
static KB_HOOK: AtomicIsize = AtomicIsize::new(0);
static MS_HOOK: AtomicIsize = AtomicIsize::new(0);
static TX: OnceLock<Sender<Input>> = OnceLock::new();
static CFG: OnceLock<(bool, Duration)> = OnceLock::new(); // (remote is right of Windows, hold time)

pub fn unhook() {
    for h in [&KB_HOOK, &MS_HOOK] {
        let v = h.swap(0, Ordering::SeqCst);
        if v != 0 { unsafe { let _ = UnhookWindowsHookEx(HHOOK(v as _)); } }
    }
    SUPPRESS.store(false, Ordering::SeqCst);
}

fn send(i: Input) { if let Some(tx) = TX.get() { let _ = tx.send(i); } }

/// Blocks running the message loop on the calling thread.
pub fn run(tx: Sender<Input>, remote_right: bool, hold: Duration) {
    TX.set(tx).ok();
    CFG.set((remote_right, hold)).ok();
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: w!("ctp-capture"), ..Default::default() };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(Default::default(), w!("ctp-capture"), w!(""), Default::default(), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst.into()), None).unwrap();
        let dev = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        RegisterRawInputDevices(&[dev], size_of::<RAWINPUTDEVICE>() as u32).unwrap();
        KB_HOOK.store(SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), Some(hinst.into()), 0).unwrap().0 as isize, Ordering::SeqCst);
        MS_HOOK.store(SetWindowsHookExW(WH_MOUSE_LL, Some(ms_hook), Some(hinst.into()), 0).unwrap().0 as isize, Ordering::SeqCst);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    unhook();
}

static mut EDGE_SINCE: Option<Instant> = None;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        unsafe {
            let mut raw = RAWINPUT::default();
            let mut size = size_of::<RAWINPUT>() as u32;
            if GetRawInputData(HRAWINPUT(lp.0 as _), RID_INPUT, Some(&mut raw as *mut _ as _), &mut size, size_of::<RAWINPUTHEADER>() as u32) != u32::MAX
                && raw.header.dwType == RIM_TYPEMOUSE.0
            {
                let m = raw.data.mouse;
                let (dx, dy) = (m.lLastX, m.lLastY);
                let flags = m.Anonymous.Anonymous.usButtonFlags as u32;
                if SUPPRESS.load(Ordering::SeqCst) {
                    if dx != 0 || dy != 0 { send(Input::Move(dx, dy)); }
                    for (i, (down, up)) in [(RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP), (RI_MOUSE_RIGHT_BUTTON_DOWN, RI_MOUSE_RIGHT_BUTTON_UP), (RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP)].iter().enumerate() {
                        if flags & *down != 0 { send(Input::Button { mask: 1 << i, down: true }); }
                        if flags & *up != 0 { send(Input::Button { mask: 1 << i, down: false }); }
                    }
                    if flags & RI_MOUSE_WHEEL != 0 {
                        let notches = (m.Anonymous.Anonymous.usButtonData as i16) / 120;
                        send(Input::Wheel(notches.clamp(-127, 127) as i8));
                    }
                } else {
                    check_edge(dx);
                }
            }
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

const RI_MOUSE_LEFT_BUTTON_DOWN: u32 = 0x0001;
const RI_MOUSE_LEFT_BUTTON_UP: u32 = 0x0002;
const RI_MOUSE_RIGHT_BUTTON_DOWN: u32 = 0x0004;
const RI_MOUSE_RIGHT_BUTTON_UP: u32 = 0x0008;
const RI_MOUSE_MIDDLE_BUTTON_DOWN: u32 = 0x0010;
const RI_MOUSE_MIDDLE_BUTTON_UP: u32 = 0x0020;
const RI_MOUSE_WHEEL: u32 = 0x0400;

/// Primary monitor only (spike). Pushing into the edge for `hold` enters the remote.
#[allow(static_mut_refs)]
unsafe fn check_edge(dx: i32) {
    let (remote_right, hold) = *CFG.get().unwrap();
    let mut p = POINT::default();
    unsafe {
        if GetCursorPos(&mut p).is_err() { return }
        let w = GetSystemMetrics(SM_CXSCREEN);
        let at_edge = if remote_right { p.x >= w - 1 && dx > 0 } else { p.x <= 0 && dx < 0 };
        if !at_edge { EDGE_SINCE = None; return }
        let since = *EDGE_SINCE.get_or_insert_with(Instant::now);
        if since.elapsed() >= hold {
            EDGE_SINCE = None;
            SUPPRESS.store(true, Ordering::SeqCst);
            send(Input::Enter);
        }
    }
}

static mut ESC_TIMES: [Option<Instant>; 3] = [None; 3];

#[allow(static_mut_refs)]
unsafe extern "system" fn kb_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && SUPPRESS.load(Ordering::SeqCst) {
            let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let vk = k.vkCode as u16;
            if down {
                let ctrl = GetAsyncKeyState(0x11) < 0;
                let alt = GetAsyncKeyState(0x12) < 0;
                if ctrl && alt && vk == 0x51 { send(Input::Escape); return LRESULT(1); }
                if vk == VK_ESCAPE.0 {
                    ESC_TIMES.rotate_left(1);
                    ESC_TIMES[2] = Some(Instant::now());
                    if ESC_TIMES[0].is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                        ESC_TIMES = [None; 3];
                        send(Input::Escape);
                        return LRESULT(1);
                    }
                }
            }
            send(Input::Key { vk, down });
            return LRESULT(1);
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

unsafe extern "system" fn ms_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && SUPPRESS.load(Ordering::SeqCst) { return LRESULT(1); }
        CallNextHookEx(None, code, wp, lp)
    }
}

use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
