use crate::message::Message;
use arboard::Clipboard;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver;
use uuid::Uuid;

pub struct Follower {
    receiver: Receiver<()>,
    sender: Sender<Result<Message, tonic::Status>>,
    state: Status,
    id: String,
}

pub enum Status {
    WORKING,
    STOPPED,
}

impl Follower {
    pub fn new(sender: Sender<Result<Message, tonic::Status>>, receiver: Receiver<()>) -> Self {
        Self {
            receiver,
            sender,
            state: Status::WORKING,
            id: Uuid::new_v4().to_string(),
        }
    }

    pub fn is_working(&self) -> bool {
        matches!(self.state, Status::WORKING)
    }

    pub fn changed(&mut self) {
        if let Ok(_) = self.receiver.try_recv() {
            self.state = Status::STOPPED;
            return;
        }

        let mut clipboard = Clipboard::new().unwrap();
        if let Ok(text) = clipboard.get_text() {
            let sender = self.sender.clone();
            sender
                .blocking_send(Ok(Message {
                    r#type: "text".to_owned(),
                    body: text.into_bytes(),
                }))
                .ok();
        }
    }

    pub fn id(&self) -> &String {
        &self.id
    }
}
