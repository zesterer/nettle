use nettle::{Node, mem, PrivateId, PublicId};
use rand::prelude::*;
use std::{borrow::Cow, fs::File, sync::Arc, collections::HashSet};

struct Graph {
    nodes: Vec<Arc<Node<mem::Mem>>>,
}

impl<'a> dot::Labeller<'a, PublicId, [PublicId; 2]> for Graph {
    fn graph_id(&'a self) -> dot::Id<'a> {
        dot::Id::new("network").unwrap()
    }

    fn node_id(&'a self, n: &PublicId) -> dot::Id<'a> {
        dot::Id::new(n.human_readable_name(2)).unwrap()
    }

    fn node_label<'b>(&'b self, n: &PublicId) -> dot::LabelText<'b> {
        dot::LabelText::label(n.human_readable_name(2))
    }

    fn edge_label<'b>(&'b self, [a, b]: &[PublicId; 2]) -> dot::LabelText<'b> {
        dot::LabelText::label(a.resource_id().dist_to(b.resource_id()).level().to_string())
    }

    fn kind(&self) -> dot::Kind { dot::Kind::Graph }
}

impl<'a> dot::GraphWalk<'a, PublicId, [PublicId; 2]> for Graph {
    fn nodes(&self) -> dot::Nodes<'a, PublicId> {
        Cow::Owned(self.nodes.iter().map(|n| n.id()).collect())
    }

    fn edges(&'a self) -> dot::Edges<'a, [PublicId; 2]> {
        Cow::Owned(self.nodes.iter()
            .flat_map(|n| n
                .get_peers()
                .into_iter()
                .map(|p| [n.id(), p])
                .map(|[a, b]| if a.resource_id() > b.resource_id() { [b, a] } else { [a, b] }))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect())
    }

    fn source(&self, e: &[PublicId; 2]) -> PublicId { e[0].clone() }

    fn target(&self, e: &[PublicId; 2]) -> PublicId { e[1].clone() }
}

#[tokio::test]
async fn discovery() {
    let spawn_node = |peers: Vec<mem::Addr>| async move {
        let private_id = PrivateId::generate();
        let addr: mem::Addr = Default::default();
        let node = Node::<mem::Mem>::new(
            private_id,
            addr.clone(),
            peers,
            addr.clone(),
        )
            .await
            .unwrap();
        let task = tokio::task::spawn(node.clone().run());
        (node, addr)
    };

    let mut nodes = vec![spawn_node(Vec::new()).await];
    for _ in 0..50 {
        let (parent, parent_port) = nodes.iter().choose(&mut thread_rng()).unwrap();
        nodes.push(spawn_node(vec![parent_port.clone()]).await);
    }

    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    dot::render(
        &Graph { nodes: nodes.into_iter().map(|n| n.0).collect() },
        &mut File::create("graph.dot").unwrap(),
    ).unwrap();
}
