use crate::clipboard::{CallbackResult, ClipboardHandler, ClipboardType, Master};
use crate::listener::follower::Follower;
use crate::message::Message;
use arboard::Clipboard;
use image::{DynamicImage, ImageBuffer, ImageFormat};
use std::fs::File;
use std::io::BufWriter;
use std::sync::mpsc::Receiver;

pub struct ClipboardListener {
    followers: Vec<Follower>,
    receiver: Receiver<String>,
    follower_receiver: Receiver<Follower>,
    clipboard: Clipboard,
}

impl ClipboardHandler for ClipboardListener {
    fn on_clipboard_change(&mut self, r#type: ClipboardType) -> CallbackResult {
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

        let message = match r#type {
            ClipboardType::IMAGE => {
                if let Ok(image) = self.clipboard.get_image() {
                    if image.bytes.len() > 10 * 1024 * 1024 {
                        println!("image is too large: {}", image.bytes.len());
                        return CallbackResult::Next;
                    }

                    // test
                    // if let Some(buffer) = ImageBuffer::from_vec(
                    //     image.width as u32,
                    //     image.height as u32,
                    //     image.bytes.to_vec(),
                    // ) {
                    //     let image = DynamicImage::ImageRgba8(buffer);
                    //     let file = File::create(
                    //         "/Users/zlatan/code/RustroverProjects/LanClip/target/debug/image.png",
                    //     )
                    //     .unwrap();
                    //     let mut writer = BufWriter::new(file);
                    //     image.write_to(&mut writer, ImageFormat::Png).unwrap();
                    // }

                    let mut data = Vec::with_capacity(image.bytes.len() + 8);
                    data.extend_from_slice(&image.width.to_le_bytes());
                    data.extend_from_slice(&image.height.to_le_bytes());
                    data.extend_from_slice(&image.bytes);

                    Ok(Message {
                        r#type: "image".to_owned(),
                        body: data,
                    })
                } else {
                    println!("image is empty");
                    return CallbackResult::Next;
                }
            }
            ClipboardType::TEXT => {
                if let Ok(text) = self.clipboard.get_text() {
                    Ok(Message {
                        r#type: "text".to_owned(),
                        body: text.into_bytes(),
                    })
                } else {
                    return CallbackResult::Next;
                }
            }
            _ => return CallbackResult::Next,
        };

        match self.receiver.try_recv() {
            Ok(id) => {
                for follower in self.followers.iter_mut() {
                    if *follower.id() != id {
                        follower.send(message.clone());
                    }
                }
            }
            Err(_) => {
                for follower in self.followers.iter_mut() {
                    follower.send(message.clone());
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
            clipboard: Clipboard::new().unwrap(),
        }
    }

    pub fn start(self) {
        Master::new(self).unwrap().run().unwrap();
    }
}
