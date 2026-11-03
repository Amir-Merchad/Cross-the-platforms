pub mod hid;
pub mod edge;
pub mod names;
pub mod transfer;
pub mod ffi;

uniffi::setup_scaffolding!();
pub mod layout;
pub mod settings;
pub mod router;
pub mod keyqueue;
pub mod clipboard;
pub mod identity;
