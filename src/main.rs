use nettle::{http, Error, Node, PrivateId};

#[tokio::main]
async fn main() -> Result<(), Error<http::Error>> {
    Node::<http::Http>::new(
        PrivateId::generate(),
        "http://[::1]:34093".parse().unwrap(),
        Vec::new(),
        http::Config {
            bind_addr: "[::1]:34093".parse().unwrap(),
        },
    )
    .await?
    .run()
    .await
}
