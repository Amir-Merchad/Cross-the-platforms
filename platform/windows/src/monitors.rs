//! Monitor enumeration for the layout (ids are the stable `\\.\DISPLAYn` device names).

use bridge_core::layout::{Monitor, Rect};
use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW},
    },
    core::BOOL,
};

pub fn list() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe extern "system" fn each(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        unsafe {
            let out = &mut *(data.0 as *mut Vec<Monitor>);
            let mut info = MONITORINFOEXW { monitorInfo: MONITORINFO { cbSize: size_of::<MONITORINFOEXW>() as u32, ..Default::default() }, ..Default::default() };
            if GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                let r = info.monitorInfo.rcMonitor;
                let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
                out.push(Monitor {
                    id: String::from_utf16_lossy(&info.szDevice[..len]),
                    rect: Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
                });
            }
            BOOL(1)
        }
    }
    unsafe { let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut out as *mut _ as isize)); }
    out
}
