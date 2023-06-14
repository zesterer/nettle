use nettle::{Node, http, PrivateId, PublicId};

#[tokio::test]
async fn discovery() {
    let spawn_node = |port, peers: Vec<(PublicId, u16)>| async move {
        let private_id = PrivateId::generate();
        let public_id = private_id.to_public();
        let task = tokio::task::spawn(Node::<http::Http>::new(
            private_id,
            format!("http://[::1]:{port}").parse().unwrap(),
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

    let node_a = spawn_node(37000, Vec::new()).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let node_b = spawn_node(37001, vec![(node_a.clone(), 37000)]).await;
    let node_c = spawn_node(37002, vec![(node_a.clone(), 37000)]).await;

    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
}
