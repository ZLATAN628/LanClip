use crate::clipboard::{CallbackResult, ClipboardHandler, ClipboardType, Master};
use crate::message::clipboard_service_client::ClipboardServiceClient;
use crate::message::Message;
use arboard::{Clipboard, ImageData};
use once_cell::sync::OnceCell;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;

static CLIPBOARD_LOCK: OnceCell<AtomicBool> = OnceCell::new();
static STATE_LOCK: OnceCell<AtomicBool> = OnceCell::new();

pub struct ClipboardClient {
    host: String,
}

struct Handler {
    sender: tokio::sync::mpsc::Sender<ClipboardType>,
}

impl Handler {
    pub fn new(sender: tokio::sync::mpsc::Sender<ClipboardType>) -> Self {
        Self { sender }
    }
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self, r#type: ClipboardType) -> CallbackResult {
        if !STATE_LOCK.get().unwrap().load(Ordering::SeqCst) {
            return CallbackResult::Stop;
        }

        if let Err(e) = self.sender.blocking_send(r#type) {
            eprintln!("send failed: {}", e);
        }
        CallbackResult::Next
    }
}

impl ClipboardClient {
    pub fn new(host: &str) -> Self {
        CLIPBOARD_LOCK.set(AtomicBool::new(false)).ok();
        STATE_LOCK.set(AtomicBool::new(true)).ok();
        Self {
            host: host.to_string(),
        }
    }

    pub async fn start(self) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = ClipboardServiceClient::connect(format!("http://{}", self.host)).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let outbound = ReceiverStream::new(rx);
        let mut stream = client.changed(outbound).await?.into_inner();
        println!("successful connected to: {}", self.host);
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<ClipboardType>(1);
        let close_notifier = Arc::new(Notify::new());

        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();
            while let Ok(Some(msg)) = stream.message().await {
                ClipboardClient::deal_message(&mut clipboard, msg);
            }
            println!("connection closed");
            STATE_LOCK.get().unwrap().store(false, Ordering::SeqCst);
        });

        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();
            while let Some(clipboard_type) = receiver.recv().await {
                if let Some(lock) = CLIPBOARD_LOCK.get() {
                    if !lock.load(Ordering::SeqCst) {
                        match clipboard_type {
                            ClipboardType::TEXT => {
                                if let Ok(text) = clipboard.get_text() {
                                    let tx = tx.clone();
                                    tokio::spawn(async move {
                                        tx.send(Message {
                                            r#type: "text".to_owned(),
                                            body: text.into_bytes(),
                                        })
                                        .await
                                        .ok();
                                    });
                                }
                            }
                            ClipboardType::IMAGE => {
                                if let Ok(image) = clipboard.get_image() {
                                    if image.bytes.len() > 10 * 1024 * 1024 {
                                        println!("image is too large: {}", image.bytes.len());
                                        return;
                                    }
                                    let mut data = Vec::with_capacity(image.bytes.len() + 8);
                                    data.extend_from_slice(&image.width.to_le_bytes());
                                    data.extend_from_slice(&image.height.to_le_bytes());
                                    data.extend_from_slice(&image.bytes);

                                    let tx = tx.clone();
                                    tokio::spawn(async move {
                                        tx.send(Message {
                                            r#type: "image".to_owned(),
                                            body: data,
                                        })
                                        .await
                                        .ok();
                                    });
                                }
                            }
                            _ => {}
                        }
                    } else {
                        lock.store(false, Ordering::SeqCst);
                    }
                }
            }
        });

        let close_notifier_clone = close_notifier.clone();
        std::thread::spawn(move || {
            let mut master = Master::new(Handler::new(sender)).unwrap();
            master.run().unwrap();
            close_notifier_clone.notify_one();
        });

        close_notifier.notified().await;
        Ok(())
    }

    fn deal_message(clipboard: &mut Clipboard, msg: Message) {
        match msg.r#type.as_ref() {
            "text" => {
                if let Ok(text) = String::from_utf8(msg.body) {
                    if let Some(lock) = CLIPBOARD_LOCK.get() {
                        lock.store(true, Ordering::SeqCst);
                        clipboard.set_text(text).ok();
                    }
                }
            }
            "image" => {
                let w: [u8; 4] = msg.body[0..4].to_vec().try_into().unwrap();
                let h: [u8; 4] = msg.body[4..8].to_vec().try_into().unwrap();
                let width = u32::from_le_bytes(w) as usize;
                let height = u32::from_le_bytes(h) as usize;
                let image_data = ImageData {
                    width,
                    height,
                    bytes: Cow::from(&msg.body[8..]),
                };
                clipboard.set_image(image_data).ok();
            }
            _ => {
                println!("not supported type: {}", msg.r#type);
            }
        }
    }
}
