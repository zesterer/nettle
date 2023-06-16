pub mod http;
pub mod mem;

use crate::{
    msg::{self, Msg},
    Node,
};

use std::{fmt, hash::Hash, sync::Arc};

#[async_trait::async_trait]
pub trait Backend: Sized + Sync + 'static {
    type Addr: Clone + Hash + Eq + fmt::Debug;
    type Config;
    type Error: fmt::Debug + Send + Sync;

    async fn create(config: Self::Config) -> Result<Self, Self::Error>;
    async fn init(&self, _node: &Arc<Node<Self>>) {}
    async fn host(node: Arc<Node<Self>>) -> Result<(), Self::Error>;
}

pub trait BackendFull = Backend
    + Sender<msg::Greet<Self>>
    + Sender<msg::Ping<Self>>
    + Sender<msg::Discover<Self>>
    + Sender<msg::Locate<Self>>
    + Sender<msg::Upload<Self>>
    + Sender<msg::Download<Self>>;

#[async_trait::async_trait]
pub trait Sender<M: Msg<Self>>: Backend {
    async fn send(&self, addr: &Self::Addr, msg: M) -> Result<M::Resp, Self::Error>;
}
