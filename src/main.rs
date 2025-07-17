use crate::message::clipboard_service_client::ClipboardServiceClient;
use crate::message::clipboard_service_server::ClipboardServiceServer;
use crate::message::Message;
use arboard::Clipboard;
use clap::{Parser, Subcommand};
use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

pub mod message {
    include!(concat!(env!("OUT_DIR"), "/message.rs"));
}

struct ServerHandler {
    clipboard: Clipboard,
    sender: tokio::sync::mpsc::Sender<Result<Message, Status>>,
}

impl ServerHandler {
    pub fn new(sender: tokio::sync::mpsc::Sender<Result<Message, Status>>) -> Self {
        Self {
            clipboard: Clipboard::new().unwrap(),
            sender,
        }
    }
}

impl ClipboardHandler for ServerHandler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if let Ok(text) = self.clipboard.get_text() {
            let sender = self.sender.clone();
            tokio::task::spawn(async move {
                sender
                    .send(Ok(Message {
                        r#type: "text".to_owned(),
                        body: text.into_bytes(),
                    }))
                    .await
                    .ok();
            });
        }
        CallbackResult::Next
    }
}

struct ClientHandler {
    clipboard: Clipboard,
    sender: tokio::sync::mpsc::Sender<Message>,
}

impl ClientHandler {
    pub fn new(sender: tokio::sync::mpsc::Sender<Message>) -> Self {
        Self {
            clipboard: Clipboard::new().unwrap(),
            sender,
        }
    }
}

impl ClipboardHandler for ClientHandler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        if let Ok(text) = self.clipboard.get_text() {
            let sender = self.sender.clone();
            tokio::task::spawn(async move {
                sender
                    .send(Message {
                        r#type: "text".to_owned(),
                        body: text.into_bytes(),
                    })
                    .await
                    .ok();
            });
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
        let mut stream = request.into_inner();

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let mut clipboard = Clipboard::new().unwrap();

            while let Ok(Some(msg)) = stream.message().await {
                deal_message(&mut clipboard, msg);
            }
        });

        tokio::spawn(async move {
            let mut master = Master::new(ServerHandler::new(tx)).unwrap();
            master.run().unwrap();
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

fn deal_message(clipboard: &mut Clipboard, msg: Message) {
    match msg.r#type.as_ref() {
        "text" => {
            if let Ok(text) = String::from_utf8(msg.body) {
                clipboard.set_text(text).ok();
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
    let mut stream = client.changed(outbound).await?.into_inner();

    tokio::spawn(async move {
        let mut clipboard = Clipboard::new().unwrap();
        while let Ok(Some(msg)) = stream.message().await {
            deal_message(&mut clipboard, msg);
        }
    });

    tokio::spawn(async move {
        let handler = ClientHandler::new(tx);
        let mut master = Master::new(handler).unwrap();
        master.run().unwrap();
    });

    tokio::signal::ctrl_c().await?;

    Ok(())
}

async fn start_server(port: i32) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    Server::builder()
        .add_service(ClipboardServiceServer::new(ClipboardServiceImpl::default()))
        .serve(addr.parse()?)
        .await?;

    Ok(())
}
