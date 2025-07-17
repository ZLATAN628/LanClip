use crate::message::clipboard_service_client::ClipboardServiceClient;
use crate::message::clipboard_service_server::ClipboardServiceServer;
use crate::message::Message;
use arboard::Clipboard;
use clap::{Parser, Subcommand};
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc::{Receiver, Sender};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

pub mod message {
    include!(concat!(env!("OUT_DIR"), "/message.rs"));
}

static CLIPBOARD_LOCK: OnceCell<AtomicBool> = OnceCell::new();

struct Handler {
    sender: Sender<bool>,
}

impl Handler {
    pub fn new(sender: Sender<bool>) -> Self {
        Self { sender }
    }
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        let sender = self.sender.clone();
        if let Err(e) = sender.blocking_send(true) {
            eprintln!("send failed: {}", e);
        }
        CallbackResult::Next
    }
}

#[derive(Default)]
pub struct ClipboardServiceImpl {}

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
        let stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        accept_remote_message(stream);

        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        listen_changed_server(tx, receiver);
        start_clipboard_listener(sender);

        Ok(Response::new(ReceiverStream::new(rx)))
    }
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

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Server {
        #[arg(short, long)]
        port: i32,
    },
    Client {
        #[arg(short, long)]
        addr: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    CLIPBOARD_LOCK.set(AtomicBool::new(false)).unwrap();
    let cli = Cli::parse();
    match cli.command {
        Command::Client { addr } => start_client(&addr).await,
        Command::Server { port } => start_server(port).await,
    }
}

async fn start_client(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ClipboardServiceClient::connect(format!("http://{}", host)).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let outbound = ReceiverStream::new(rx);
    let stream = client.changed(outbound).await?.into_inner();

    println!("successful connected to: {}", host);

    accept_remote_message(stream);

    let (sender, receiver) = tokio::sync::mpsc::channel::<bool>(1);
    listen_changed_client(tx, receiver);
    start_clipboard_listener(sender);

    tokio::signal::ctrl_c().await?;

    Ok(())
}

fn accept_remote_message(mut stream: Streaming<Message>) {
    tokio::spawn(async move {
        let mut clipboard = Clipboard::new().unwrap();
        while let Ok(Some(msg)) = stream.message().await {
            deal_message(&mut clipboard, msg);
        }
    });
}

fn start_clipboard_listener(sender: Sender<bool>) {
    std::thread::spawn(move || {
        let mut master = Master::new(Handler::new(sender)).unwrap();
        master.run().unwrap();
    });
}

fn listen_changed_client(sender: Sender<Message>, mut receiver: Receiver<bool>) {
    tokio::spawn(async move {
        let mut clipboard = Clipboard::new().unwrap();
        while let Some(true) = receiver.recv().await {
            if let Some(lock) = CLIPBOARD_LOCK.get() {
                if !lock.load(Ordering::SeqCst) {
                    if let Ok(text) = clipboard.get_text() {
                        let sender = sender.clone();
                        tokio::spawn(async move {
                            sender
                                .send(Message {
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
}

fn listen_changed_server(
    sender: tokio::sync::mpsc::Sender<Result<Message, Status>>,
    mut receiver: Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut clipboard = Clipboard::new().unwrap();
        while let Some(true) = receiver.recv().await {
            if let Some(lock) = CLIPBOARD_LOCK.get() {
                if !lock.load(Ordering::SeqCst) {
                    if let Ok(text) = clipboard.get_text() {
                        let sender = sender.clone();
                        tokio::spawn(async move {
                            sender
                                .send(Ok(Message {
                                    r#type: "text".to_owned(),
                                    body: text.into_bytes(),
                                }))
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
}

async fn start_server(port: i32) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    Server::builder()
        .add_service(ClipboardServiceServer::new(ClipboardServiceImpl::default()))
        .serve(addr.parse()?)
        .await?;

    Ok(())
}
