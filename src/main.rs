mod client;
mod listener;
mod server;

use crate::client::ClipboardClient;
use crate::listener::{ClipboardListener, Follower};
use crate::message::clipboard_service_server::ClipboardServiceServer;
use crate::server::server::ClipboardServiceImpl;
use clap::{Parser, Subcommand};
use tonic::transport::Server;

pub mod message {
    include!(concat!(env!("OUT_DIR"), "/message.rs"));
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
    ClipboardClient::new(host).start().await
}

async fn start_server(port: i32) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let (ftx, frx) = std::sync::mpsc::channel::<Follower>();

    std::thread::spawn(move || {
        let listener = ClipboardListener::new(rx, frx);
        listener.start();
    });

    Server::builder()
        .add_service(ClipboardServiceServer::new(ClipboardServiceImpl::new(
            tx, ftx,
        )))
        .serve(addr.parse()?)
        .await?;

    Ok(())
}
