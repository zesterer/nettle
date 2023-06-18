use crate::{Backend, PublicId, Tag};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;

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
    // Ok(true) => I own the resource
    // Ok(false) => I do not own the resource and do not know anybody closer to the resource (404!)
    // Err(_) => I do not own the resource but this other node is closer to it
    pub result: Result<bool, (PublicId, B::Addr)>,
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

/// Upload some data to the network.
#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct Download<B: Backend> {
    pub tag: Tag,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "B::Addr: Serialize",
    deserialize = "B::Addr: DeserializeOwned"
))]
pub struct DownloadResp<B: Backend> {
    // Some(_) => I own the resource and here it is
    // None => I do not own the resource
    pub data: Option<Box<[u8]>>,
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}
