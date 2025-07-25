mod common;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::{Master, Shutdown};

pub use common::{CallbackResult, ClipboardHandler, ClipboardType};
