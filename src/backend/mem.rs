use crate::{msg, Backend, Node, PublicId, Tag};
use std::{cmp, fmt, hash, sync::Arc, sync::OnceLock, time::Duration};

#[derive(Clone, Default)]
pub struct Addr(pub Arc<OnceLock<Arc<Node<Mem>>>>);

impl hash::Hash for Addr {
    fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
        Arc::as_ptr(&self.0).hash(hasher);
    }
}
impl cmp::PartialEq for Addr {
    fn eq(&self, other: &Self) -> bool {
        Arc::as_ptr(&self.0) as *const () == Arc::as_ptr(&other.0) as *const ()
    }
}
impl cmp::Eq for Addr {}

impl fmt::Debug for Addr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<addr>")
    }
}

pub struct Mem {
    addr: Addr,
}

#[async_trait::async_trait]
impl Backend for Mem {
    type Addr = Addr;
    type Config = Addr;
    type Error = std::convert::Infallible;

    async fn create(addr: Self::Config) -> Result<Self, Self::Error> {
        Ok(Self { addr })
    }

    async fn init(&self, node: &Arc<Node<Self>>) {
        node.backend.addr.0.set(node.clone()).ok().unwrap();
    }

    async fn host(_: Arc<Node<Self>>) -> Result<(), Self::Error> {
        let () = futures::future::pending().await;
        Ok(())
    }

    async fn send_greet(
        &self,
        addr: &Self::Addr,
        sender: (PublicId, Self::Addr),
    ) -> Result<Result<PublicId, Option<Self::Addr>>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_greet(sender).await)
    }

    async fn send_ping(&self, addr: &Self::Addr) -> Result<Duration, Self::Error> {
        addr.0.get().unwrap().recv_ping().await;
        Ok(Duration::ZERO)
    }

    async fn send_discover(
        &self,
        addr: &Self::Addr,
        target: Tag,
        max_level: u16,
    ) -> Result<Option<(PublicId, Self::Addr)>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_discover(target, max_level).await)
    }

    async fn send_locate(
        &self,
        addr: &Self::Addr,
        msg: msg::Locate<Self>,
    ) -> Result<msg::LocateResp<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_locate(msg).await)
    }

    async fn send_upload(
        &self,
        addr: &Self::Addr,
        msg: msg::Upload<Self>,
    ) -> Result<msg::UploadResp<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_upload(msg).await)
    }

    async fn send_download(
        &self,
        addr: &Self::Addr,
        msg: msg::Download<Self>,
    ) -> Result<msg::DownloadResp<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_download(msg).await)
    }
}
