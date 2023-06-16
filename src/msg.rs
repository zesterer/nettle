use crate::{Backend, PublicId, Tag};

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
    pub target: Tag,
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

/// Attempt to discover a tag in the network.
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Locate<B: Backend> {
    pub tag: Tag,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct LocateResp<B: Backend> {
    // Ok(Some(_)) => I own the resource and here it is
    // Ok(None) => I do not own the resource and do not know anybody closer to the resource (404!)
    // Err(_) => I do not own the resource but this other node is closer to it
    pub result: Result<Option<Box<[u8]>>, (PublicId, B::Addr)>,
}

impl<B: Backend> Msg<B> for Locate<B> {
    type Resp = LocateResp<B>;
}

/// Upload some data to the network.
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Upload<B: Backend> {
    pub data: Box<[u8]>,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct UploadResp<B: Backend> {
    pub result: Result<Tag, ()>,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

impl<B: Backend> Msg<B> for Upload<B> {
    type Resp = UploadResp<B>;
}
