#![feature(trait_alias, let_chains)]

mod backend;
mod msg;
mod identity;

pub use crate::{
    backend::{http, mem},
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

const MAX_LEVEL_PEERS: usize = 1;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    pub fn level(&self) -> u16 {
        self.0.into_iter().enumerate()
            .find_map(|(i, b)| if b != 0 {
                Some(255 - i as u16 * 8 - b.ilog2() as u16)
            } else {
                None
            })
            .unwrap_or(0)
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
    peers_by_level: [Vec<PeerIdx>; 256],
}

pub struct Node<B: Backend> {
    self_id: PrivateId,
    self_addr: B::Addr,
    initial_peers: Vec<B::Addr>,
    backend: B,
    state: Mutex<State<B>>,
}

impl<B: BackendFull> Node<B> {
    pub async fn new(
        self_id: PrivateId,
        self_addr: B::Addr,
        initial_peers: Vec<B::Addr>,
        config: B::Config,
    ) -> Result<Arc<Self>, Error<B::Error>> {
        let this = Self {
            self_id,
            self_addr,
            initial_peers,
            backend: B::create(config).await.map_err(Error::Backend)?,
            state: Mutex::new(State {
                peers: SlotMap::default(),
                peers_by_id: HashMap::default(),
                peers_by_level: {
                    const EMPTY: Vec<PeerIdx> = Vec::new();
                    [EMPTY; 256]
                },
            }),
        };
        let this = Arc::new(this);
        this.backend.init(&this).await;
        Ok(this)
    }

    pub fn id(&self) -> PublicId {
        self.self_id.to_public()
    }

    pub fn addr(&self) -> &B::Addr {
        &self.self_addr
    }

    pub fn get_peers(&self) -> Vec<PublicId> {
        self.with_state(|state| state.peers.values().map(|p| p.id.clone()).collect())
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
                        state.peers_by_level[level as usize].push(idx);
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
                state.peers_by_level[level as usize].retain(|idx| idx != &peer_idx);
            }
        });
    }

    pub fn can_accept_peer(&self, id: &PublicId) -> bool {
        id != &self.self_id.to_public() && self.with_state(|state| {
            let level = self.self_id.to_public().resource_id().dist_to(id.resource_id()).level();
            state.peers_by_level[level as usize].len() < MAX_LEVEL_PEERS && !state.peers_by_id.contains_key(id)
        })
    }

    pub async fn discover_peer(&self, supposed_id: Option<&PublicId>, addr: B::Addr) -> Result<(), Option<B::Addr>> {
        if supposed_id.map_or(true, |sid| self.can_accept_peer(&sid)) {
            match self.backend
                .send(&addr, msg::Greet { sender: (self.self_id.to_public(), self.self_addr.clone()) })
                .await
            {
                Ok(resp) => match resp.result {
                    Ok(id) if supposed_id.map_or(true, |sid| sid == &id) => {
                        eprintln!("{:?} discovered accepting peer {:?}!", self.self_id, id);
                        self.accept_peer(id, addr).await;
                        Ok(())
                    },
                    Ok(id) => {
                        eprintln!("{:?} peer got a different ID ({:?}) to the ID it was reported ({:?})!", self.self_id, id, supposed_id);
                        Err(None)
                    },
                    Err(alt) => {
                        // eprintln!("{:?} was rejected by peer {:?}.", self.self_id, supposed_id);
                        Err(alt)
                    },
                },
                Err(err) => {
                    eprintln!("Failed to sent greeting to initial peer: {:?}", err);
                    Err(None)
                },
            }
        } else {
            Err(None)
        }
    }

    pub async fn recv_greet(&self, greet: msg::Greet<B>) -> msg::GreetResp<B> {
        // If we're willing to
        if self.can_accept_peer(&greet.sender.0) {
            eprintln!("{:?} accepted peer {:?}!", self.self_id, greet.sender.0);
            self.accept_peer(greet.sender.0, greet.sender.1).await;
            msg::GreetResp { result: Ok(self.id()), phantom: Default::default() }
        } else {
            msg::GreetResp {
                // Choose one of our existing peers to have the greeter talk to instead
                // ("I don't want to be friends with you, go ask that other person")
                result: Err(self.with_state(|state| state.peers
                    .values()
                    .choose(&mut thread_rng())
                    .map(|peer| peer.addr.clone()))),
                phantom: Default::default(),
            }
        }
    }

    pub async fn recv_ping(&self, ping: msg::Ping<B>) -> msg::Pong<B> {
        msg::Pong { phantom: Default::default() }
    }

    pub async fn recv_discover(&self, discover: msg::Discover<B>) -> msg::DiscoverResp<B> {
        msg::DiscoverResp {
            // Determine whether we have a peer within at given distance
            peer: self.with_state(|state| state.peers
                .values()
                // Don't tell the peer about itself
                .filter(|peer| peer.id != discover.target)
                // Only consider peers that are closer than the target
                .filter(|peer| peer.id.resource_id().dist_to(discover.target.resource_id()).level() <= discover.max_level)
                // .choose(&mut thread_rng())
                // // Try to find that which has the greatest distance within the maximum distance
                .min_by_key(|peer| peer.id.resource_id().dist_to(discover.target.resource_id()).0)
                // // Only pass that peer on about a third of the time
                // .filter(|_| thread_rng().gen_bool(0.3))
                .map(|peer| (peer.id.clone(), peer.addr.clone()))),
        }
    }

    pub async fn fetch_data(&self, id: ResourceId) -> Result<Vec<u8>, ()> {
        todo!("Fetch data with ID {:?}", id)
    }

    pub async fn run(self: Arc<Self>) -> Result<(), Error<B::Error>> {
        let mut host = tokio::task::spawn(B::host(self.clone()));

        eprintln!("Starting node `{:?}`", self.self_id);

        // Automatically discover all initial peers
        for mut peer_addr in self.initial_peers.iter().cloned() {
            loop {
                match self.discover_peer(None, peer_addr.clone()).await {
                    Ok(()) => break,
                    Err(None) => {
                        eprintln!("{:?} failed to peer with initial peer {:?}!", self.id(), peer_addr);
                    },
                    Err(Some(alt_addr)) => {
                        eprintln!("{:?} attempted to connect to initial peer, but was rejected. Peer suggested {:?} instead.", self.id(), alt_addr);
                        peer_addr = alt_addr;
                    },
                }
            }
        }

        let mut ping = tokio::time::interval(Duration::from_secs(3));
        let mut discover = tokio::time::interval(Duration::from_secs(1));

        loop {
            select! {
                res = &mut host => break res.unwrap().map_err(Error::Backend),
                _ = ping.tick() => {
                    for (peer_idx, peer) in self.with_state(|state| state.peers
                        .iter()
                        .map(|(idx, peer)| (idx, peer.addr.clone()))
                        .collect::<Vec<_>>())
                    {
                        match self.backend
                            .send(&peer, msg::Ping { phantom: Default::default() })
                            .await
                        {
                            Ok(_) => {},
                            Err(err) => {
                                eprintln!("Failed to sent ping to peer, removing from list: {:?}", err);
                                self.remove_peer(peer_idx).await;
                            },
                        }
                    }
                },
                _ = discover.tick() => {
                    if let Some(mut current_peer) = self.with_state(|state| state.peers
                        .values()
                        .choose(&mut thread_rng())
                        .map(|peer| (peer.id.clone(), peer.addr.clone())))
                    {
                        for current_level in (0..256).rev() {
                            match self.backend
                                .send(&current_peer.1, msg::Discover {
                                    target: self.id(),
                                    max_level: current_level,
                                    phantom: Default::default(),
                                })
                                .await
                            {
                                Ok(resp) => if let Some((id, addr)) = resp.peer {
                                    let _ = self.discover_peer(Some(&id), addr.clone()).await;
                                    current_peer = (id, addr);
                                } else {
                                    break // Trail has gone cold
                                },
                                Err(err) => eprintln!("Failed to sent discover to peer: {:?}", err),
                            }
                        }
                    }


                    /*
                    if let Some((peer_id, peer_addr)) = self.with_state(|state| state.peers
                        .values()
                        .choose(&mut thread_rng())
                        .map(|peer| (peer.id.clone(), peer.addr.clone())))
                    {
                        // Don't ask a peer for an unreasonable target distance that it's exceedingly unlikely to attain
                        let max_ask_level = peer_id.resource_id().dist_to(self.id().resource_id()).level().saturating_sub(2);
                        match self.backend
                            .send(&peer_addr, msg::Discover {
                                target: self.id(),
                                max_level: self.with_state(|state| state
                                    .peers_by_level
                                    .iter()
                                    .enumerate()
                                    .filter(|(level, _)| *level as u16 >= max_ask_level)
                                    .map(|(level, peers)| (level as u16, peers.len()))
                                    .collect::<Vec<_>>()
                                    .choose_weighted(&mut thread_rng(), |(level, len)| (*level as f32).powf(2.0) / (1.0 + *len as f32))
                                    .unwrap().0),
                                phantom: Default::default(),
                            })
                            .await
                        {
                            Ok(resp) => {
                                if let Some((id, addr)) = resp.peer {
                                    let _ = self.discover_peer(Some(&id), addr).await;
                                }
                            },
                            Err(err) => eprintln!("Failed to sent discover to peer: {:?}", err),
                        }
                    }
                    */
                },
            }
        }
    }
}
