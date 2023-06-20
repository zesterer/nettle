use clap::Parser;
use nettle::{http, Error, Node, PrivateId};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    initial_peers: Vec<String>,
    #[arg(short, long, default_value = "[::1]")]
    address: String,
    #[arg(short, long)]
    url: Option<String>,
    #[arg(short, long, default_value_t = 34093)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Error<http::Error>> {
    let args = Args::parse();

    let host_addr = if let Some(url) = args.url {
        url
    } else {
        let public_ip = public_ip_addr::get_public_ip()
            .await
            .expect("failed to get public IP");
        format!("http://{}:{}", public_ip, args.port)
    };
    let host_url = host_addr.parse().unwrap();
    println!("Using {} as the host URL", host_url);

    Node::<http::Http>::new(
        PrivateId::generate(),
        host_url,
        args.initial_peers,
        http::Config {
            bind_addr: format!("{}:{}", args.address, args.port).parse().unwrap(),
        },
    )
    .await?
    .run()
    .await
}
