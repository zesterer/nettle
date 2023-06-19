pub mod http;
pub mod mem;

use crate::{Node, PublicId, Tag};

use std::{fmt, hash::Hash, sync::Arc, time::Duration};

#[async_trait::async_trait]
pub trait Backend: Sized + Sync + 'static {
    type Addr: Clone + Hash + Eq + fmt::Debug;
    type Config;
    type Error: fmt::Debug + Send + Sync;

    async fn create(config: Self::Config) -> Result<Self, Self::Error>;
    async fn init(&self, _node: &Arc<Node<Self>>) {}
    async fn host(node: Arc<Node<Self>>) -> Result<(), Self::Error>;

    async fn send_greet(
        &self,
        addr: &Self::Addr,
        sender: (PublicId, Self::Addr),
    ) -> Result<Result<PublicId, Option<Self::Addr>>, Self::Error>;
    async fn send_ping(&self, addr: &Self::Addr) -> Result<Duration, Self::Error>;
    async fn send_discover(
        &self,
        addr: &Self::Addr,
        target: Tag,
        max_level: u16,
    ) -> Result<Option<(PublicId, Self::Addr)>, Self::Error>;
    async fn send_locate(
        &self,
        addr: &Self::Addr,
        tag: Tag,
    ) -> Result<Result<bool, (PublicId, Self::Addr)>, Self::Error>;
    async fn send_upload(
        &self,
        addr: &Self::Addr,
        data: Box<[u8]>,
    ) -> Result<Result<(), ()>, Self::Error>;
    async fn send_download(
        &self,
        addr: &Self::Addr,
        tag: Tag,
    ) -> Result<Option<Box<[u8]>>, Self::Error>;
}
