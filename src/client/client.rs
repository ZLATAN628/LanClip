use crate::message::clipboard_service_client::ClipboardServiceClient;
use crate::message::Message;
use arboard::Clipboard;
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;

static CLIPBOARD_LOCK: OnceCell<AtomicBool> = OnceCell::new();

pub struct ClipboardClient {
    host: String,
}

struct Handler {
    sender: tokio::sync::mpsc::Sender<()>,
}

impl Handler {
    pub fn new(sender: tokio::sync::mpsc::Sender<()>) -> Self {
        Self { sender }
    }
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if let Err(e) = self.sender.blocking_send(()) {
            eprintln!("send failed: {}", e);
        }
        CallbackResult::Next
    }
}

impl ClipboardClient {
    pub fn new(host: &str) -> Self {
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
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<()>(1);
        let mut master = Master::new(Handler::new(sender)).unwrap();
        let close_channel = master.shutdown_channel();

        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();
            while let Ok(Some(msg)) = stream.message().await {
                ClipboardClient::deal_message(&mut clipboard, msg);
            }
            close_channel.signal();
            println!("connection closed");
        });

        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();
            while let Some(_) = receiver.recv().await {
                if let Some(lock) = CLIPBOARD_LOCK.get() {
                    if !lock.load(Ordering::SeqCst) {
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
                    } else {
                        lock.store(false, Ordering::SeqCst);
                    }
                }
            }
        });

        std::thread::spawn(move || {
            master.run().unwrap();
        });
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
            _ => {
                println!("not supported type: {}", msg.r#type);
            }
        }
    }
}
