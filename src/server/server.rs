use crate::listener::Follower;
use crate::message;
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

#[derive(Clone)]
pub struct ClipboardServiceImpl {
    sender: std::sync::mpsc::Sender<String>,
    follower_sender: std::sync::mpsc::Sender<Follower>,
}

impl ClipboardServiceImpl {
    pub fn new(
        sender: std::sync::mpsc::Sender<String>,
        follower_sender: std::sync::mpsc::Sender<Follower>,
    ) -> Self {
        Self {
            sender,
            follower_sender,
        }
    }
}

#[tonic::async_trait]
impl message::clipboard_service_server::ClipboardService for ClipboardServiceImpl {
    type ChangedStream = ReceiverStream<Result<message::Message, Status>>;

    async fn changed(
        &self,
        request: Request<Streaming<message::Message>>,
    ) -> Result<Response<Self::ChangedStream>, Status> {
        if let Some(addr) = request.remote_addr() {
            println!(
                "new connection received: {}:{}",
                addr.ip().to_string(),
                addr.port()
            );
        }
        let mut stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let (otx, orx) = tokio::sync::oneshot::channel::<()>();

        let follower = Follower::new(tx, orx);
        let sender = self.sender.clone();
        let id = follower.id().clone();
        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();
            while let Ok(Some(msg)) = stream.message().await {
                match msg.r#type.as_ref() {
                    "text" => {
                        if let Ok(text) = String::from_utf8(msg.body) {
                            sender.send(id.clone()).ok();
                            clipboard.set_text(text).ok();
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
            otx.send(()).ok();
            println!("connection closed");
        });

        self.follower_sender.send(follower).ok();

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
