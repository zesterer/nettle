#![feature(trait_alias)]

mod backend;
mod msg;
mod identity;

pub use crate::{
    backend::http,
    identity::{PublicId, PrivateId},
};

use crate::backend::{Backend, BackendFull, Sender};

use slotmap::SlotMap;
use tokio::select;
use rand::prelude::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
    collections::HashMap,
};

pub type DataId = u128;

#[derive(Debug)]
pub enum Error<B> {
    Backend(B),
}

slotmap::new_key_type! { struct PeerId; }

struct Peer<B: Backend> {
    id: PublicId,
    addr: B::Addr,
}

pub struct State<B: Backend> {
    peers: SlotMap<PeerId, Peer<B>>,
    peers_by_id: HashMap<PublicId, PeerId>,
}

pub struct Node<B: Backend> {
    backend: B,
    self_id: PrivateId,
    self_addr: B::Addr,
    state: Mutex<State<B>>,
}

impl<B: BackendFull> Node<B> {
    pub async fn new(
        self_id: PrivateId,
        self_addr: B::Addr,
        initial_peers: Vec<(PublicId, B::Addr)>,
        config: B::Config,
    ) -> Result<Self, Error<B::Error>> {
        let this = Self {
            self_id,
            self_addr,
            backend: B::start(config).await.map_err(Error::Backend)?,
            state: Mutex::new(State {
                peers: SlotMap::default(),
                peers_by_id: HashMap::default(),
            }),
        };
        for (id, addr) in initial_peers {
            this.accept_peer(id, addr).await;
        }
        Ok(this)
    }

    pub fn with_state<F: FnOnce(&mut State<B>) -> R, R>(&self, f: F) -> R {
        f(&mut self.state.lock().unwrap())
    }

    pub async fn accept_peer(&self, id: PublicId, addr: B::Addr) {
        if id != self.self_id.to_public() {
            self.with_state(|state| {
                state.peers_by_id
                    .entry(id.clone())
                    .or_insert_with(|| {
                        eprintln!("{:?} discovered peer {:?}!", self.self_id, id);
                        state.peers.insert(Peer { id, addr })
                    });
            });
        }
    }

    pub async fn recv_greet(&self, greet: msg::Greet<B>) -> msg::GreetResp<B> {
        self.accept_peer(greet.sender.0, greet.sender.1).await;
        msg::GreetResp { phantom: Default::default() }
    }

    pub async fn recv_ping(&self, ping: msg::Ping<B>) -> msg::Pong<B> {
        msg::Pong { phantom: Default::default() }
    }

    pub async fn recv_discover(&self, ping: msg::Discover<B>) -> msg::DiscoverResp<B> {
        msg::DiscoverResp { peer: self.with_state(|state| state.peers
                .values()
                .choose(&mut thread_rng())
                .map(|peer| (peer.id.clone(), peer.addr.clone()))) }
    }

    pub async fn fetch_data(&self, id: DataId) -> Result<Vec<u8>, ()> {
        todo!("Fetch data with ID {}", id)
    }

    pub async fn run(self) -> Result<(), Error<B::Error>> {
        let node = Arc::new(self);
        let mut host = tokio::task::spawn(B::host(node.clone()));

        eprintln!("Starting node `{:?}`", node.self_id);

        // Send a greeting to all initial peers
        for peer in node.with_state(|state| state.peers
            .values()
            .map(|peer| peer.addr.clone())
            .collect::<Vec<_>>())
        {
            if let Err(err) = node.backend
                .send(&peer, msg::Greet { sender: (node.self_id.to_public(), node.self_addr.clone()) })
                .await
            {
                eprintln!("Failed to sent greeting to initial peer: {:?}", err);
            }
        }

        let mut ping = tokio::time::interval(Duration::from_secs(3));
        let mut discover = tokio::time::interval(Duration::from_secs(5));

        loop {
            select! {
                res = &mut host => break res.unwrap().map_err(Error::Backend),
                _ = ping.tick() => {
                    for peer in node.with_state(|state| state.peers
                        .values()
                        .map(|peer| peer.addr.clone())
                        .collect::<Vec<_>>())
                    {
                        match node.backend
                            .send(&peer, msg::Ping { phantom: Default::default() })
                            .await
                        {
                            Ok(_) => {},//eprintln!("`{:?}` received pong from `{:?}`!", node.self_addr, peer),
                            Err(err) => eprintln!("Failed to sent ping to initial peer: {:?}", err),
                        }
                    }
                },
                _ = discover.tick() => {
                    if let Some(peer) = node.with_state(|state| state.peers
                        .values()
                        .choose(&mut thread_rng())
                        .map(|peer| peer.addr.clone()))
                    {
                        match node.backend
                            .send(&peer, msg::Discover { phantom: Default::default() })
                            .await
                        {
                            Ok(resp) => {
                                if let Some((id, addr)) = resp.peer {
                                    node.accept_peer(id, addr).await;
                                }
                            },
                            Err(err) => eprintln!("Failed to sent ping to initial peer: {:?}", err),
                        }
                    }
                },
            }
        }
    }
}
