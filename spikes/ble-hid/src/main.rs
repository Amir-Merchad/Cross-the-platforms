//! Spikes 2–4: Windows as BLE keyboard+mouse, per-host targeting, edge switching. Throwaway.
//!
//!   ble-hid hosts                       list subscribed host ids
//!   ble-hid demo [--target ID] [--clicks]   square + "hello" every 5 s
//!   ble-hid edge --target ID [--remote-at right|left] [--width 1180] [--gain 1.0] [--threshold 60]
//!
//! Back to Windows: Ctrl+Alt+Q, Esc ×3 within 1 s, pushing back through the edge, host disconnect.

mod capture;
mod keys;
mod peripheral;

use std::{sync::{atomic::Ordering, mpsc}, thread, time::{Duration, Instant}};

use bridge_core::{edge::{EdgeEvent, EdgeTracker, Side}, hid::{HeldInput, MouseReport, split_delta}};
use capture::Input;
use peripheral::HidPeripheral;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn wait_host(p: &HidPeripheral, target: Option<String>) -> String {
    eprintln!("advertising; pair this PC from the device's Bluetooth settings…");
    loop {
        let hosts = p.hosts();
        if let Some(t) = &target { if hosts.contains(t) { return t.clone() } }
        else if let Some(h) = hosts.into_iter().next() { return h }
        thread::sleep(Duration::from_millis(500));
    }
}

fn send_move(p: &HidPeripheral, host: &str, buttons: u8, dx: i32, dy: i32) {
    for (x, y) in split_delta(dx, dy) { p.send_mouse(host, MouseReport { buttons, dx: x, dy: y, ..Default::default() }); }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("demo");
    let p = HidPeripheral::start().expect("BLE HID start failed (quit Universal Bridge first)");

    match mode {
        // Start, confirm advertising, then stop cleanly (Drop stops advertising).
        "check" => {
            thread::sleep(Duration::from_secs(5));
            eprintln!("hosts: {:?}; stopping cleanly", p.hosts());
        }
        "hosts" => loop {
            eprintln!("hosts: {:?}", p.hosts());
            thread::sleep(Duration::from_secs(2));
        },
        "demo" => {
            let host = wait_host(&p, arg(&args, "--target"));
            eprintln!("host ready: {host}");
            let clicks = args.iter().any(|a| a == "--clicks");
            let mut held = HeldInput::default();
            loop {
                for (dx, dy) in [(200, 0), (0, 200), (-200, 0), (0, -200)] {
                    for _ in 0..20 { send_move(&p, &host, 0, dx / 20, dy / 20); thread::sleep(Duration::from_millis(10)); }
                }
                if clicks {
                    p.send_mouse(&host, MouseReport { buttons: held.button(1, true), ..Default::default() });
                    p.send_mouse(&host, MouseReport { buttons: held.button(1, false), ..Default::default() });
                    p.send_mouse(&host, MouseReport { wheel: -3, ..Default::default() });
                }
                for vk in [0x48u16, 0x45, 0x4C, 0x4C, 0x4F] { // h e l l o
                    p.send_keyboard(&host, held.key_down(vk));
                    p.send_keyboard(&host, held.key_up(vk));
                }
                thread::sleep(Duration::from_secs(5));
            }
        }
        "edge" => edge(&p, &args),
        other => eprintln!("unknown mode {other}"),
    }
}

fn edge(p: &HidPeripheral, args: &[String]) {
    let host = wait_host(p, arg(args, "--target"));
    let remote_right = arg(args, "--remote-at").as_deref() != Some("left");
    let width: f64 = arg(args, "--width").and_then(|v| v.parse().ok()).unwrap_or(1180.0);
    let gain: f64 = arg(args, "--gain").and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let threshold: f64 = arg(args, "--threshold").and_then(|v| v.parse().ok()).unwrap_or(60.0);
    // remote to the right of Windows → its left edge touches Windows
    let mut tracker = EdgeTracker::new(if remote_right { Side::Left } else { Side::Right }, width, gain, threshold);

    std::panic::set_hook(Box::new(|info| { capture::unhook(); eprintln!("{info}"); }));
    let (tx, rx) = mpsc::channel::<Input>();
    thread::spawn(move || capture::run(tx, remote_right, Duration::from_millis(300)));
    eprintln!("host {host}: push the pointer through the {} edge", if remote_right { "right" } else { "left" });

    let interval = Duration::from_millis(p.report_interval_ms(&host));
    eprintln!("motion report interval: {} ms (from BLE connection interval)", interval.as_millis());
    let mut held = HeldInput::default();
    let mut last_check = Instant::now();
    let release = |held: &mut HeldInput| {
        // Windows input first: the BLE sends below can block on a dying link.
        capture::SUPPRESS.store(false, Ordering::SeqCst);
        let (k, m) = held.release_all();
        p.send_keyboard(&host, k);
        p.send_mouse(&host, m);
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let w = GetSystemMetrics(SM_CXSCREEN);
            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetCursorPos(if remote_right { w - 20 } else { 20 }, pt.y);
        }
        let (n, avg, max) = p.take_stats();
        eprintln!("back on Windows ({n} reports, notify avg {avg:.1} ms, max {max:.1} ms)");
    };

    // Motion is paced: accumulate deltas, send at most one report per connection interval
    // (Universal Bridge's approach). Clicks/keys/wheel go out immediately after pending motion.
    let (mut mx, mut my) = (0, 0);
    let mut next_motion = Instant::now();
    loop {
        let wait = if mx != 0 || my != 0 { next_motion.saturating_duration_since(Instant::now()) } else { Duration::from_millis(500) };
        match rx.recv_timeout(wait) {
            Ok(Input::Move(dx, dy)) => { mx += dx; my += dy; }
            Ok(other) => {
                flush(p, &host, &mut tracker, &mut held, &mut mx, &mut my, &release);
                match other {
                    Input::Enter => {
                        tracker.enter();
                        send_move(p, &host, 0, tracker.pin_delta(), 0);
                        let _ = p.take_stats();
                        eprintln!("on device");
                    }
                    // after an escape, queued remote events are stale: drop them
                    Input::Button { .. } | Input::Key { .. } | Input::Wheel(_) if !capture::SUPPRESS.load(Ordering::SeqCst) => {}
                    Input::Button { mask, down } => { p.send_mouse(&host, MouseReport { buttons: held.button(mask, down), ..Default::default() }); }
                    Input::Wheel(w) => { p.send_mouse(&host, MouseReport { buttons: held.button(0, true), wheel: w, ..Default::default() }); }
                    Input::Key { vk, down } => { p.send_keyboard(&host, if down { held.key_down(vk) } else { held.key_up(vk) }); }
                    Input::Escape => release(&mut held),
                    Input::Move(..) => unreachable!(),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if (mx != 0 || my != 0) && Instant::now() >= next_motion {
            flush(p, &host, &mut tracker, &mut held, &mut mx, &mut my, &release);
            next_motion = Instant::now() + interval;
        }
        if last_check.elapsed() > Duration::from_millis(500) {
            last_check = Instant::now();
            if capture::SUPPRESS.load(Ordering::SeqCst) && !p.hosts().contains(&host) { release(&mut held); }
        }
    }
}

fn flush(p: &HidPeripheral, host: &str, t: &mut EdgeTracker, held: &mut HeldInput, mx: &mut i32, my: &mut i32, release: &impl Fn(&mut HeldInput)) {
    if *mx == 0 && *my == 0 || !capture::SUPPRESS.load(Ordering::SeqCst) { *mx = 0; *my = 0; return }
    if t.on_move(*mx) == EdgeEvent::Return { release(held); } else {
        let b = held.button(0, true); // empty mask: reads current buttons so drags keep the button held
        send_move(p, host, b, *mx, *my);
    }
    *mx = 0; *my = 0;
}
