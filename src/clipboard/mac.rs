/**
* Copyright (c) 2015 Douman
*/
use std::io;
use std::sync::mpsc::{self, sync_channel, Receiver, SyncSender};

use crate::clipboard::common::{CallbackResult, ClipboardHandler, ClipboardType};
use objc2::rc::Retained;
use objc2::{msg_send, ClassType};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF,
};

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

///Shutdown channel
///
///On drop requests shutdown to gracefully close clipboard listener as soon as possible.
pub struct Shutdown {
    sender: SyncSender<()>,
}

impl Drop for Shutdown {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = self.sender.send(());
    }
}

///Clipboard master.
///
///Tracks changes of clipboard and invokes corresponding callbacks.
///
///# Platform notes:
///
///- On `windows` it creates dummy window that monitors each clipboard change message.
pub struct Master<H> {
    handler: H,
    sender: SyncSender<()>,
    recv: Receiver<()>,
}

impl<H: ClipboardHandler> Master<H> {
    #[inline(always)]
    ///Creates new instance.
    pub fn new(handler: H) -> io::Result<Self> {
        let (sender, recv) = sync_channel(0);

        Ok(Self {
            handler,
            sender,
            recv,
        })
    }

    #[inline(always)]
    ///Creates shutdown channel.
    pub fn shutdown_channel(&self) -> Shutdown {
        Shutdown {
            sender: self.sender.clone(),
        }
    }

    ///Starts Master by polling clipboard for change
    pub fn run(&mut self) -> io::Result<()> {
        let pasteboard: Option<Retained<NSPasteboard>> =
            unsafe { msg_send![NSPasteboard::class(), generalPasteboard] };

        let pasteboard = match pasteboard {
            Some(pasteboard) => pasteboard,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Unable to create mac pasteboard",
                ))
            }
        };

        let mut prev_count = unsafe { pasteboard.changeCount() };
        let mut result = Ok(());

        loop {
            let count: isize = unsafe { pasteboard.changeCount() };

            if count == prev_count {
                match self.recv.recv_timeout(self.handler.sleep_interval()) {
                    Ok(()) => break,
                    //timeout
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }

            prev_count = count;

            let mut clipboard_type = ClipboardType::UNKNOWN;
            if let Some(items) = unsafe { pasteboard.pasteboardItems() } {
                if let Some(item) = items.firstObject() {
                    unsafe {
                        let types = item.types();
                        if types.containsObject(NSPasteboardTypeTIFF)
                        {
                            clipboard_type = ClipboardType::IMAGE
                        } else if types.containsObject(NSPasteboardTypeString) {
                            clipboard_type = ClipboardType::TEXT
                        }
                    }
                }
            }
            match self.handler.on_clipboard_change(clipboard_type) {
                CallbackResult::Next => (),
                CallbackResult::Stop => break,
                CallbackResult::StopWithError(error) => {
                    result = Err(error);
                    break;
                }
            }

            match self.recv.try_recv() {
                Ok(()) => break,
                Err(mpsc::TryRecvError::Empty) => continue,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        result
    }
}
