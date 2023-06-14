use nettle::{Node, http, PrivateId, PublicId};
use tokio_stream::StreamExt;

#[tokio::test]
async fn discovery() {
    let spawn_node = |mut port, peers: Vec<(PublicId, u16)>| async move {
        let private_id = PrivateId::generate();
        let public_id = private_id.to_public();
        let mut sock: std::net::SocketAddr = format!("[::1]:{port}").parse().unwrap();
        while std::net::TcpListener::bind(sock).is_err() {
            port += 1;
            sock.set_port(port);
        }
        let task = tokio::task::spawn(Node::<http::Http>::new(
            private_id,
            format!("http://[::1]:{port}"),
            peers
                .into_iter()
                .map(|(id, port)| (id, format!("http://[::1]:{port}").parse().unwrap()))
                .collect(),
            http::Config { bind_addr: format!("[::1]:{port}").parse().unwrap() },
        )
            .await
            .unwrap()
            .run());
        public_id
    };

    let node_root = spawn_node(37000, Vec::new()).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let nodes = tokio_stream::iter(0..100)
        .then(|i| spawn_node(47001 + i as u16, vec![(node_root.clone(), 37000)]))
        .collect::<Vec<_>>()
        .await;

    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
}
