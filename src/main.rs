use clap::Parser;
use nettle::{http, Error, Node, PrivateId};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    initial_peers: Vec<String>,
    #[arg(short, long, default_value = "[::1]")]
    address: String,
    #[arg(short, long, default_value_t = 34093)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Error<http::Error>> {
    let args = Args::parse();

    let host_addr = format!("http://{}:{}", args.address, args.port);
    Node::<http::Http>::new(
        PrivateId::generate(),
        host_addr.parse().unwrap(),
        args.initial_peers,
        http::Config {
            bind_addr: format!("{}:{}", args.address, args.port).parse().unwrap(),
        },
    )
    .await?
    .run()
    .await
}
