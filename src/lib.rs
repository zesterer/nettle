#![feature(trait_alias, let_chains)]
#![deny(warnings)]

mod backend;
mod identity;
mod msg;
mod tag;

pub use crate::{
    backend::{http, mem},
    identity::{PrivateId, PublicId},
    tag::Tag,
};

use crate::backend::{Backend, BackendFull, Sender};

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
}

pub struct State<B: Backend> {
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

    pub fn with_state<F: FnOnce(&mut State<B>) -> R, R>(&self, f: F) -> R {
        f(&mut self.state.lock().unwrap())
    }

    pub async fn accept_peer(&self, id: PublicId, addr: B::Addr) -> bool {
        if id != self.self_id.pub_id
            && !self.with_state(|state| state.peers_by_id.contains_key(&id))
            && let Ok(_) = self.backend
                .send(&addr, msg::Ping { phantom: Default::default() })
                .await
        {
            let level = self.self_id.pub_id.tag.dist_to(id.tag).level();
            self.with_state(|state| {
                state.peers_by_id
                    .entry(id.clone())
                    .or_insert_with(|| {
                        let idx = state.peers.insert(Peer { id, addr });
                        state.peers_by_level[level as usize].push(idx);
                        idx
                    });
            });
            true
        } else {
            false
        }
    }

    async fn remove_peer(&self, peer_idx: PeerIdx) {
        self.with_state(|state| {
            if let Some(peer) = state.peers.remove(peer_idx) {
                let level = self.self_id.pub_id.tag.dist_to(peer.id.tag).level();
                state.peers_by_id.remove(&peer.id);
                state.peers_by_level[level as usize].retain(|idx| idx != &peer_idx);
            }
        });
    }

    pub fn can_accept_peer(&self, id: &PublicId) -> bool {
        id != &self.self_id.pub_id
            && self.with_state(|state| {
                let level = self.self_id.pub_id.tag.dist_to(id.tag).level();
                state.peers_by_level[level as usize].len() < MAX_LEVEL_PEERS
                    && !state.peers_by_id.contains_key(id)
            })
    }

    pub async fn discover_peer(
        &self,
        supposed_id: Option<&PublicId>,
        addr: B::Addr,
    ) -> Result<(), Option<B::Addr>> {
        if supposed_id.map_or(true, |sid| self.can_accept_peer(&sid)) {
            match self
                .backend
                .send(
                    &addr,
                    msg::Greet {
                        sender: (self.self_id.pub_id.clone(), self.self_addr.clone()),
                    },
                )
                .await
            {
                Ok(resp) => {
                    match resp.result {
                        Ok(id) if supposed_id.map_or(true, |sid| sid == &id) => {
                            eprintln!("{:?} discovered accepting peer {:?}!", self.self_id, id);
                            self.accept_peer(id, addr).await;
                            Ok(())
                        }
                        Ok(id) => {
                            eprintln!("{:?} peer got a different ID ({:?}) to the ID it was reported ({:?})!", self.self_id, id, supposed_id);
                            Err(None)
                        }
                        Err(alt) => {
                            // eprintln!("{:?} was rejected by peer {:?}.", self.self_id, supposed_id);
                            Err(alt)
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Failed to sent greeting to initial peer: {:?}", err);
                    Err(None)
                }
            }
        } else {
            Err(None)
        }
    }

    pub async fn recv_greet(&self, greet: msg::Greet<B>) -> msg::GreetResp<B> {
        // If we're willing to
        if self.can_accept_peer(&greet.sender.0)
            && self
                .accept_peer(greet.sender.0.clone(), greet.sender.1)
                .await
        {
            eprintln!("{:?} accepted peer {:?}!", self.self_id, greet.sender.0);
            msg::GreetResp {
                result: Ok(self.id().clone()),
                phantom: Default::default(),
            }
        } else {
            msg::GreetResp {
                // Choose one of our existing peers to have the greeter talk to instead
                // ("I don't want to be friends with you, go ask that other person")
                result: Err(self.with_state(|state| {
                    state
                        .peers
                        .values()
                        .choose(&mut thread_rng())
                        .map(|peer| peer.addr.clone())
                })),
                phantom: Default::default(),
            }
        }
    }

    pub async fn recv_ping(&self, _ping: msg::Ping<B>) -> msg::Pong<B> {
        msg::Pong {
            phantom: Default::default(),
        }
    }

    pub async fn recv_discover(&self, discover: msg::Discover<B>) -> msg::DiscoverResp<B> {
        msg::DiscoverResp {
            // Determine whether we have a peer within at given distance
            peer: self.with_state(|state| {
                state
                    .peers
                    .values()
                    // Don't tell the peer about itself
                    .filter(|peer| peer.id.tag != discover.target)
                    // Only consider peers that are closer than the target
                    .filter(|peer| {
                        peer.id.tag.dist_to(discover.target).level() <= discover.max_level
                    })
                    .choose(&mut thread_rng())
                    // // Try to find that which has the greatest distance within the maximum distance
                    // .min_by_key(|peer| peer.id.tag.dist_to(discover.target.tag))
                    // // Only pass that peer on about a third of the time
                    // .filter(|_| thread_rng().gen_bool(0.3))
                    .map(|peer| (peer.id.clone(), peer.addr.clone()))
            }),
        }
    }

    pub async fn load_data(&self, tag: Tag) -> Option<Box<[u8]>> {
        Some(
            self.with_state(|state| state.data.get(&tag).cloned())?
                .to_vec()
                .into_boxed_slice(),
        )
    }

    pub async fn save_data(&self, tag: Tag, data: Box<[u8]>) {
        let data = data.to_vec().into();
        self.with_state(|state| {
            state.data.entry(tag).or_insert(data);
        })
    }

    pub async fn recv_upload(&self, upload: msg::Upload<B>) -> msg::UploadResp<B> {
        let tag = Tag::digest(&*upload.data);
        msg::UploadResp {
            // Try to find a peer that's closer to upload to
            result: if let Some(closer) = self.with_state(|state| {
                state
                    .peers
                    .values()
                    .filter(|peer| peer.id.tag.dist_to(tag) < self.id().tag.dist_to(tag))
                    .min_by_key(|peer| peer.id.tag.dist_to(tag))
                    .map(|peer| peer.addr.clone())
            }) {
                match self.backend.send(&closer, upload).await {
                    Ok(resp) => resp.result,
                    Err(err) => {
                        eprintln!(
                            "{:?} failed to sent locate to peer {:?}: {:?}",
                            self.id(),
                            closer,
                            err
                        );
                        Err(())
                    }
                }
            } else {
                self.save_data(tag, upload.data).await;
                Ok(tag)
            },
            phantom: Default::default(),
        }
    }

    pub async fn recv_locate(&self, locate: msg::Locate<B>) -> msg::LocateResp<B> {
        msg::LocateResp {
            result: match self.load_data(locate.tag).await {
                // If we have the data, return it
                Some(data) => Ok(Some(data)),
                // If we don't have the data, attempt to find someone closer to it
                None => {
                    let self_dist = self.id().tag.dist_to(locate.tag);
                    self.with_state(|state| {
                        state
                            .peers
                            .values()
                            .filter(|peer| peer.id.tag.dist_to(locate.tag) < self_dist)
                            .min_by_key(|peer| peer.id.tag.dist_to(locate.tag))
                            .map(|peer| (peer.id.clone(), peer.addr.clone()))
                    })
                    .map(Err)
                    .unwrap_or(Ok(None))
                }
            },
        }
    }

    pub async fn fetch_data(&self, tag: Tag) -> Result<Option<Box<[u8]>>, ()> {
        // Find the peer with the closest tag
        if let Some(data) = self.load_data(tag).await {
            Ok(Some(data))
        } else if let Some(mut closest) = self.with_state(|state| {
            state
                .peers
                .values()
                .min_by_key(|peer| peer.id.tag.dist_to(tag))
                .map(|peer| (peer.id.clone(), peer.addr.clone()))
        }) {
            loop {
                match self
                    .backend
                    .send(
                        &closest.1,
                        msg::Locate {
                            tag,
                            phantom: Default::default(),
                        },
                    )
                    .await
                {
                    Ok(resp) => match resp.result {
                        Ok(Some(data)) => break Ok(Some(data)),
                        // TODO: Maybe we should do backtracking here or something?
                        Ok(None) => break Ok(None),
                        Err(next_closest) => {
                            if next_closest.0.tag.dist_to(tag) < closest.0.tag.dist_to(tag) {
                                // Not found yet, but we have another link to follow
                                closest = next_closest;
                            } else {
                                // We found a liar! Peer returned a node that was further. Assume this means that it can't
                                // locate it.
                                eprintln!("{:?} lied to peer {:?} and returned a node that was *further* from the target!", closest.0, self.id());
                                break Ok(None);
                            }
                        }
                    },
                    Err(err) => {
                        eprintln!(
                            "{:?} failed to sent locate to peer {:?}: {:?}",
                            self.id(),
                            closest.0,
                            err
                        );
                        break Err(());
                    }
                }
            }
        } else {
            Ok(None)
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
                        match self.backend
                            .send(&peer, msg::Ping { phantom: Default::default() })
                            .await
                        {
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
                                .send(&current_peer.1, msg::Discover {
                                    target: self.id().tag,
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
                },
            }
        }
    }
}
