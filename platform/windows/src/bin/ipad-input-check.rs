//! Device check for the iPad input service, with no engine or UI: connects over Wi-Fi like the app,
//! then draws a square with the pointer, right-clicks, scrolls and types "hello" into whatever has
//! focus (open Spotlight or Notes first). Quit the app first so only one process drives the iPad.
//!
//!   cargo run -p ctp-windows --bin ipad-input-check
//!
//! States go to stderr; the test text is fixed, nothing typed by the owner is read or logged.

use std::{sync::mpsc, time::Duration};

use bridge_core::hid::{KeyboardReport, MouseReport};
use ctp_windows::ipadinput::{self, IpadCommand, IpadInputState};

fn main() {
    let (tx, rx) = mpsc::channel();
    let service = ipadinput::start(ctp_windows::ipad::ipad_dir(), move |s| { eprintln!("state: {s:?}"); let _ = tx.send(s); });
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(IpadInputState::Ready) => break,
            Ok(_) => {}
            Err(_) => { eprintln!("FAILED: not ready within 60 s"); return }
        }
    }
    let pause = |ms| std::thread::sleep(Duration::from_millis(ms));
    let mouse = |r: MouseReport| service.send(IpadCommand::Mouse(r));
    service.send(IpadCommand::Enter);
    eprintln!("square: watch the iPad pointer");
    for (dx, dy) in [(16, 0), (0, 16), (-16, 0), (0, -16)] {
        for _ in 0..30 { mouse(MouseReport { dx, dy, ..Default::default() }); pause(8); }
    }
    pause(500);
    eprintln!("right click: a context menu may open");
    mouse(MouseReport { buttons: 2, ..Default::default() });
    pause(60);
    mouse(MouseReport::default());
    pause(1000);
    eprintln!("Esc, then scroll down 3 notches and back");
    let key = |k: u8| {
        service.send(IpadCommand::Keyboard(KeyboardReport { modifiers: 0, keys: [k, 0, 0, 0, 0, 0] }));
        pause(30);
        service.send(IpadCommand::Keyboard(KeyboardReport::default()));
        pause(30);
    };
    key(0x29);
    for wheel in [-16, -16, -16, 16, 16, 16] { mouse(MouseReport { wheel, ..Default::default() }); pause(120); }
    eprintln!("typing \"hello\" into the focused field");
    for k in [0x0B, 0x08, 0x0F, 0x0F, 0x12] { key(k) }
    pause(500);
    eprintln!("holding Shift and a, then leaving: nothing must stay held");
    service.send(IpadCommand::Keyboard(KeyboardReport { modifiers: 0x02, keys: [0x04, 0, 0, 0, 0, 0] }));
    pause(100);
    service.send(IpadCommand::Leave);
    pause(1000);
    drop(service);
    eprintln!("finished: check the iPad shows a square, a menu, scrolling, \"helloA\" and no repeating key");
}
