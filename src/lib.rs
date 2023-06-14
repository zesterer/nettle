#![feature(trait_alias, let_chains)]

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
use serde::{Serialize, Deserialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
    collections::HashMap,
};

const MAX_LEVEL_PEERS: usize = 5;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct ResourceId([u8; 32]);

impl ResourceId {
    pub fn dist_to(&self, other: Self) -> Self {
        let mut dist = self.0;
        for i in 0..dist.len() {
            dist[i] ^= other.0[i];
        }
        Self(dist)
    }

    // log2, effectively
    pub fn level(&self) -> usize {
        self.0.into_iter().enumerate()
            .find_map(|(i, b)| if b != 0 {
                Some(255 - i * 8 - b.ilog2() as usize)
            } else {
                None
            })
            .unwrap_or_else(|| {
                eprintln!("Hash collision with peer?!");
                0
            })
    }
}

#[derive(Debug)]
pub enum Error<B> {
    Backend(B),
}

slotmap::new_key_type! { struct PeerIdx; }

struct Peer<B: Backend> {
    id: PublicId,
    addr: B::Addr,
}

pub struct State<B: Backend> {
    peers: SlotMap<PeerIdx, Peer<B>>,
    peers_by_id: HashMap<PublicId, PeerIdx>,
    peers_by_dist: [Vec<PeerIdx>; 256],
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
                peers_by_dist: {
                    const EMPTY: Vec<PeerIdx> = Vec::new();
                    [EMPTY; 256]
                },
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
        if id != self.self_id.to_public()
            && !self.with_state(|state| state.peers_by_id.contains_key(&id))
            && let Ok(_) = self.backend
                .send(&addr, msg::Ping { phantom: Default::default() })
                .await
        {
            let level = self.self_id.to_public().resource_id().dist_to(id.resource_id()).level();
            self.with_state(|state| {
                state.peers_by_id
                    .entry(id.clone())
                    .or_insert_with(|| {
                        let idx = state.peers.insert(Peer { id, addr });
                        state.peers_by_dist[level].push(idx);
                        idx
                    });
            });
        }
    }

    async fn remove_peer(&self, peer_idx: PeerIdx) {
        self.with_state(|state| {
            if let Some(peer) = state.peers.remove(peer_idx) {
                let level = self.self_id.to_public().resource_id().dist_to(peer.id.resource_id()).level();
                state.peers_by_id.remove(&peer.id);
                state.peers_by_dist[level].retain(|idx| idx != &peer_idx);
            }
        });
    }

    pub async fn discover_peer(&self, id: PublicId, addr: B::Addr) {
        // Determine whether we have information about the peer already
        if id != self.self_id.to_public()
            && !self.with_state(|state| state.peers_by_id.contains_key(&id))
        {
            // Determine whether the peer is actually useful to us
            let level = self.self_id.to_public().resource_id().dist_to(id.resource_id()).level();
            if self.with_state(|state| state.peers_by_dist[level].len() < MAX_LEVEL_PEERS) {
                eprintln!("{:?} discovered peer {:?}!", self.self_id, id);
                // TODO: Technically a race condition with peer removal above, but ah well
                self.accept_peer(id, addr).await;
            }
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

    pub async fn fetch_data(&self, id: ResourceId) -> Result<Vec<u8>, ()> {
        todo!("Fetch data with ID {:?}", id)
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
                    for (peer_idx, peer) in node.with_state(|state| state.peers
                        .iter()
                        .map(|(idx, peer)| (idx, peer.addr.clone()))
                        .collect::<Vec<_>>())
                    {
                        match node.backend
                            .send(&peer, msg::Ping { phantom: Default::default() })
                            .await
                        {
                            Ok(_) => {},
                            Err(err) => {
                                eprintln!("Failed to sent ping to peer, removing from list: {:?}", err);
                                node.remove_peer(peer_idx).await;
                            },
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
                                    node.discover_peer(id, addr).await;
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
