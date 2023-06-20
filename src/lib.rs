#![deny(warnings)]

mod backend;
mod identity;
mod tag;

pub use crate::{
    backend::{http, mem},
    identity::{PrivateId, PublicId},
    tag::Tag,
};

use crate::backend::Backend;

use rand::prelude::*;
use slotmap::SlotMap;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::select;

const MAX_LEVEL_PEERS: usize = 2;

#[derive(Debug)]
pub enum Error<B> {
    Backend(B),
}

slotmap::new_key_type! { struct PeerIdx; }

struct Peer<B: Backend> {
    id: PublicId,
    addr: B::Addr,
    ping: Duration, // Total round trip
}

struct State<B: Backend> {
    peers: SlotMap<PeerIdx, Peer<B>>,
    peers_by_id: HashMap<PublicId, PeerIdx>,
    peers_by_level: [Vec<PeerIdx>; 256],
    data: HashMap<Tag, Arc<[u8]>>,
}

pub struct Node<B: Backend> {
    self_id: PrivateId,
    self_addr: B::Addr,
    initial_peers: Vec<B::Addr>,
    backend: B,
    state: Mutex<State<B>>,
}

impl<B: Backend> Node<B> {
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
                data: HashMap::default(),
            }),
        };
        let this = Arc::new(this);
        this.backend.init(&this).await;
        Ok(this)
    }

    pub fn id(&self) -> &PublicId {
        &self.self_id.pub_id
    }

    pub fn addr(&self) -> &B::Addr {
        &self.self_addr
    }

    pub fn get_peers(&self) -> Vec<PublicId> {
        self.with_state(|state| state.peers.values().map(|p| p.id.clone()).collect())
    }

    fn with_state<F: FnOnce(&mut State<B>) -> R, R>(&self, f: F) -> R {
        f(&mut self.state.lock().unwrap())
    }

    pub async fn accept_peer(&self, id: PublicId, addr: B::Addr) -> bool {
        if id != self.self_id.pub_id
            && !self.with_state(|state| state.peers_by_id.contains_key(&id))
        {
            if let Ok(ping) = self.backend.send_ping(&addr).await {
                let level = self.self_id.pub_id.tag.dist_to(id.tag).level();
                self.with_state(|state| {
                    state
                        .peers_by_id
                        .entry(id.clone())
                        .and_modify(|idx| state.peers[*idx].ping = ping)
                        .or_insert_with(|| {
                            let idx = state.peers.insert(Peer { id, addr, ping });
                            state.peers_by_level[level as usize].push(idx);
                            idx
                        });
                });
                true
            } else {
                eprintln!(
                    "Tried to accept peer {:?} but they did not respond to a ping",
                    id
                );
                false
            }
        } else {
            false
        }
    }

    async fn remove_peer(&self, peer_idx: PeerIdx) -> bool {
        self.with_state(|state| {
            if let Some(peer) = state.peers.remove(peer_idx) {
                let level = self.self_id.pub_id.tag.dist_to(peer.id.tag).level();
                state.peers_by_id.remove(&peer.id);
                state.peers_by_level[level as usize].retain(|idx| idx != &peer_idx);
                true
            } else {
                false
            }
        })
    }

    pub fn can_accept_peer(&self, id: &PublicId) -> bool {
        id != &self.self_id.pub_id
            && self.with_state(|state| {
                let level = self.self_id.pub_id.tag.dist_to(id.tag).level();
                state.peers_by_level[level as usize].len() < MAX_LEVEL_PEERS
                    && !state.peers_by_id.contains_key(id)
            })
    }

    // Ok(()) => discovery was successful and we're now peered with the node
    // Err(None) => discovery was unsuccessful and the node did not provide a suggested alternative peer
    // Err(Some(_)) => discovery was unsuccessful but the node gave us a suggested alternative peer to try
    pub async fn discover_peer(
        &self,
        supposed_id: Option<&PublicId>,
        addr: B::Addr,
    ) -> Result<(), Option<B::Addr>> {
        if supposed_id.map_or(true, |sid| self.can_accept_peer(&sid)) {
            match self
                .backend
                .send_greet(&addr, (self.id().clone(), self.addr().clone()))
                .await
            {
                Ok(Ok(id)) if supposed_id.map_or(true, |sid| sid == &id) => {
                    eprintln!("{:?} discovered accepting peer {:?}!", self.self_id, id);
                    self.accept_peer(id, addr).await;
                    Ok(())
                }
                Ok(Ok(id)) => {
                    eprintln!(
                        "{:?} peer got a different ID ({:?}) to the ID it was reported ({:?})!",
                        self.self_id, id, supposed_id
                    );
                    Err(None)
                }
                Ok(Err(alt)) => Err(alt),
                Err(err) => {
                    eprintln!("Failed to sent greeting to initial peer: {}", err);
                    Err(None)
                }
            }
        } else {
            eprintln!("Can't accept peer {:?}", supposed_id);
            Err(None)
        }
    }

    pub async fn recv_greet(
        &self,
        sender: (PublicId, B::Addr),
    ) -> Result<PublicId, Option<B::Addr>> {
        // If we're willing to
        if self.can_accept_peer(&sender.0) && self.accept_peer(sender.0.clone(), sender.1).await {
            eprintln!("{:?} accepted peer {:?}!", self.self_id, sender.0);
            Ok(self.id().clone())
        } else {
            // Choose one of our existing peers to have the greeter talk to instead
            // ("I don't want to be friends with you, go ask that other person")
            let alt = self.with_state(|state| {
                state
                    .peers
                    .values()
                    .choose(&mut thread_rng())
                    .map(|peer| peer.addr.clone())
            });
            eprintln!(
                "Rejected greeting from {:?}, returned alternative peer {:?}",
                sender.0, alt
            );
            Err(alt)
        }
    }

    pub async fn recv_ping(&self) {}

    pub async fn recv_discover(&self, target: Tag, max_level: u16) -> Option<(PublicId, B::Addr)> {
        // Determine whether we have a peer within at given distance
        self.with_state(|state| {
            state
                .peers
                .values()
                // Don't tell the peer about itself
                .filter(|peer| peer.id.tag != target)
                // Only consider peers that are closer than the target
                .filter(|peer| peer.id.tag.dist_to(target).level() <= max_level)
                .choose(&mut thread_rng())
                // // Try to find that which has the greatest distance within the maximum distance
                // .min_by_key(|peer| peer.id.tag.dist_to(discover.target.tag))
                // // Only pass that peer on about a third of the time
                // .filter(|_| thread_rng().gen_bool(0.3))
                .map(|peer| (peer.id.clone(), peer.addr.clone()))
        })
    }

    pub async fn load_data(&self, tag: Tag) -> Option<Box<[u8]>> {
        Some(
            self.with_state(|state| state.data.get(&tag).cloned())?
                .to_vec()
                .into_boxed_slice(),
        )
    }

    pub async fn has_data(&self, tag: Tag) -> bool {
        self.with_state(|state| state.data.contains_key(&tag))
    }

    pub async fn save_data(&self, tag: Tag, data: Box<[u8]>) {
        let data = data.to_vec().into();
        self.with_state(|state| {
            state.data.entry(tag).or_insert(data);
        })
    }

    pub async fn recv_download(&self, tag: Tag) -> Option<Box<[u8]>> {
        self.load_data(tag).await
    }

    pub async fn recv_upload(&self, data: Box<[u8]>) -> Result<(), ()> {
        let tag = Tag::digest(&*data);
        self.save_data(tag, data).await;
        Ok(())
    }

    pub async fn locate_data(&self, tag: Tag) -> Result<(bool, (PublicId, B::Addr)), &'static str> {
        if self.has_data(tag).await {
            Ok((true, (self.id().clone(), self.self_addr.clone())))
        } else if let Some(mut closest) = self.with_state(|state| {
            let self_dist = self.id().tag.dist_to(tag);
            // Find the peer with the closest tag
            state
                .peers
                .values()
                .filter(|peer| peer.id.tag.dist_to(tag) < self_dist)
                .min_by_key(|peer| peer.id.tag.dist_to(tag))
                .map(|peer| (peer.id.clone(), peer.addr.clone()))
        }) {
            loop {
                match self.backend.send_locate(&closest.1, tag).await {
                    Ok(Ok(has_data)) => break Ok((has_data, closest)),
                    Ok(Err(next_closest)) => {
                        if next_closest.0.tag.dist_to(tag) < closest.0.tag.dist_to(tag) {
                            // Not found yet, but we have another link to follow
                            closest = next_closest;
                        } else {
                            // We found a liar! Peer returned a node that was further. Assume this means that it can't
                            // locate it.
                            eprintln!("{:?} lied to {:?} and returned a node that was *further* from the target!", closest.0, self.id());
                            break Ok((false, (self.id().clone(), self.self_addr.clone())));
                        }
                    }
                    Err(_err) => break Err("peer did not respond"),
                }
            }
        } else {
            Ok((false, (self.id().clone(), self.self_addr.clone())))
        }
    }

    pub async fn recv_locate(&self, tag: Tag) -> Result<bool, (PublicId, B::Addr)> {
        if self.has_data(tag).await {
            // If we have the data, return it
            Ok(true)
        } else {
            // If we don't have the data, attempt to find someone closer to it
            let self_dist = self.id().tag.dist_to(tag);
            self.with_state(|state| {
                state
                    .peers
                    .values()
                    .filter(|peer| peer.id.tag.dist_to(tag) < self_dist)
                    .min_by_key(|peer| peer.id.tag.dist_to(tag))
                    .map(|peer| (peer.id.clone(), peer.addr.clone()))
            })
            .map(Err)
            .unwrap_or(Ok(false))
        }
    }

    pub async fn do_upload(&self, data: Box<[u8]>) -> Result<Tag, &'static str> {
        let tag = Tag::digest(&*data);
        match self.locate_data(tag).await {
            Ok((true, _)) => Ok(tag), // Already uploaded
            // We're the closest node
            Ok((false, closest)) if closest.0.tag == self.id().tag => {
                self.save_data(tag, data).await;
                Ok(tag)
            }
            // The closest node is another node
            Ok((false, closest)) => match self.backend.send_upload(&closest.1, data).await {
                Ok(resp) => resp.map(|()| tag).map_err(|()| "peer did not respond"),
                Err(_err) => Err("peer did not respond"),
            },
            Err(err) => Err(err),
        }
    }

    pub async fn do_download(&self, tag: Tag) -> Result<Option<Box<[u8]>>, &'static str> {
        match self.locate_data(tag).await? {
            (true, closest) if closest.0 == *self.id() => Ok(self.load_data(tag).await),
            (true, closest) => match self.backend.send_download(&closest.1, tag).await {
                Ok(Some(data)) if Tag::digest(&*data) == tag => Ok(Some(data)),
                Ok(Some(_)) => {
                    eprintln!("data integrity check from {:?} failed", closest.0);
                    Err("integrity check failed")
                }
                Ok(None) => Err("peer reported data but did not provide any"),
                Err(_err) => Err("peer did not respond"),
            },
            (false, _) => Ok(None),
        }
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
                        eprintln!(
                            "{:?} failed to peer with initial peer {:?}!",
                            self.id(),
                            peer_addr
                        );
                        break;
                    }
                    Err(Some(alt_addr)) => {
                        eprintln!("{:?} attempted to connect to initial peer, but was rejected. Peer suggested {:?} instead.", self.id(), alt_addr);
                        peer_addr = alt_addr;
                    }
                }
            }
        }

        let mut ping = tokio::time::interval(Duration::from_secs(10));
        let mut discover = tokio::time::interval(Duration::from_secs(5));

        loop {
            select! {
                res = &mut host => break res.unwrap().map_err(Error::Backend),
                _ = ping.tick() => {
                    for (peer_idx, peer) in self.with_state(|state| state.peers
                        .iter()
                        .map(|(idx, peer)| (idx, peer.addr.clone()))
                        .collect::<Vec<_>>())
                    {
                        match self.backend.send_ping(&peer).await {
                            Ok(_) => {},
                            Err(_) => {
                                eprintln!("Failed to sent ping to peer, removing from list.");
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
                                .send_discover(&current_peer.1, self.id().tag, current_level)
                                .await
                            {
                                Ok(Some(closest)) => if closest.0.tag.dist_to(self.id().tag).level() <= current_level {
                                    let _ = self.discover_peer(Some(&closest.0), closest.1.clone()).await;
                                    current_peer = closest;
                                } else {
                                    eprintln!("{:?} lied to peer {:?} and returned a node that was *further* from the target!", closest.0, self.id());
                                    break
                                },
                                Ok(None) => break, // Trail has gone cold
                                Err(err) => eprintln!("Failed to sent discover to peer: {:?}", err),
                            }
                        }
                    }
                },
            }
        }
    }
}
