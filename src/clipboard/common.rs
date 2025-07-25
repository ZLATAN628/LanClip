use crate::clipboard::Shutdown;
/**
* Copyright (c) 2015 Douman
*/
use std::io;

///Describes Clipboard handler
pub trait ClipboardHandler {
    ///Callback to call on clipboard change.
    fn on_clipboard_change(&mut self, r#type: ClipboardType) -> CallbackResult;
    ///Callback to call on when error happens in master.
    fn on_clipboard_error(&mut self, error: io::Error) -> CallbackResult {
        CallbackResult::StopWithError(error)
    }

    #[inline(always)]
    ///Returns sleep interval for polling implementations (e.g. Mac).
    ///
    ///Default value is 500ms
    fn sleep_interval(&self) -> core::time::Duration {
        core::time::Duration::from_millis(500)
    }
}

///Possible return values of callback.
pub enum CallbackResult {
    ///Wait for next clipboard change.
    Next,
    ///Stop handling messages.
    Stop,
    ///Special variant to propagate IO Error from callback.
    StopWithError(io::Error),
}

#[derive(Debug)]
pub enum ClipboardType {
    IMAGE,
    TEXT,
    FILE,
    UNKNOWN,
}

impl Shutdown {
    ///Signals shutdown
    pub fn signal(self) {
        drop(self);
    }
}
