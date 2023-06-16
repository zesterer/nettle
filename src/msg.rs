use crate::{Backend, PublicId};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;

pub trait Msg<B: Backend> {
    type Resp;
}

// Greet

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Greet<B: Backend> {
    pub sender: (PublicId, B::Addr),
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct GreetResp<B: Backend> {
    pub result: Result<PublicId, Option<B::Addr>>,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

impl<B: Backend> Msg<B> for Greet<B> {
    type Resp = GreetResp<B>;
}

// Ping

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Ping<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Pong<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

impl<B: Backend> Msg<B> for Ping<B> {
    type Resp = Pong<B>;
}

/// Attempt to discover a new peer by asking existing peers.
///
/// `addr` specifies the original requesting peer.
/// `max_level` specifies the maximum distance (log2) that the returned peer should be from the given address
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Discover<B: Backend> {
    pub target: PublicId,
    pub max_level: u16,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct DiscoverResp<B: Backend> {
    pub peer: Option<(PublicId, B::Addr)>,
}

impl<B: Backend> Msg<B> for Discover<B> {
    type Resp = DiscoverResp<B>;
}
