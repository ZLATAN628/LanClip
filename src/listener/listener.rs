use crate::listener::follower::Follower;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use std::sync::mpsc::Receiver;

pub struct ClipboardListener {
    followers: Vec<Follower>,
    receiver: Receiver<String>,
    follower_receiver: Receiver<Follower>,
}

impl ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self) -> CallbackResult {

        while let Ok(follower) = self.follower_receiver.try_recv() {
            self.followers.push(follower);
        }

        if self.followers.is_empty() {
            return CallbackResult::Next;
        }

        // remove stopped follower
        self.followers.retain(|f| f.is_working());

        if self.followers.is_empty() {
            return CallbackResult::Next;
        }

        match self.receiver.try_recv() {
            Ok(id) => {
                for follower in self.followers.iter_mut() {
                    if *follower.id() != id {
                        follower.changed();
                    }
                }
            }
            Err(_) => {
                for follower in self.followers.iter_mut() {
                    follower.changed();
                }
            }
        }

        CallbackResult::Next
    }
}

impl ClipboardListener {
    pub fn new(receiver: Receiver<String>, follower_receiver: Receiver<Follower>) -> Self {
        Self {
            followers: vec![],
            receiver,
            follower_receiver,
        }
    }

    pub fn start(self) {
        Master::new(self).unwrap().run().unwrap();
    }
}
