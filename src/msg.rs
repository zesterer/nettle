use crate::{PublicId, Backend};

use serde::{Serialize, Deserialize, de::DeserializeOwned};
use std::marker::PhantomData;

pub trait Msg<B: Backend>: Serialize + DeserializeOwned {
    type Resp: Serialize + DeserializeOwned;
}

// Greet

#[derive(Serialize, Deserialize)]
pub struct Greet<B: Backend> {
    pub sender: (PublicId, B::Addr),
}

#[derive(Serialize, Deserialize)]
pub struct GreetResp<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

impl<B: Backend> Msg<B> for Greet<B> {
    type Resp = GreetResp<B>;
}

// Ping

#[derive(Serialize, Deserialize)]
pub struct Ping<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
pub struct Pong<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

impl<B: Backend> Msg<B> for Ping<B> {
    type Resp = Pong<B>;
}

// Discover

#[derive(Serialize, Deserialize)]
pub struct Discover<B: Backend> {
    #[serde(skip)]
    pub phantom: PhantomData<B>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscoverResp<B: Backend> {
    pub peer: Option<(PublicId, B::Addr)>,
}

impl<B: Backend> Msg<B> for Discover<B> {
    type Resp = DiscoverResp<B>;
}
