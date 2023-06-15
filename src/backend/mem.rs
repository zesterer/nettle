use crate::{Node, Backend, Sender, msg::{self, Msg}};
use std::{sync::Arc, sync::OnceLock, fmt, cmp, hash};

#[derive(Clone, Default)]
pub struct Addr(pub Arc<OnceLock<Arc<Node<Mem>>>>);

impl hash::Hash for Addr {
    fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
        Arc::as_ptr(&self.0).hash(hasher);
    }
}
impl cmp::PartialEq for Addr {
    fn eq(&self, other: &Self) -> bool { Arc::as_ptr(&self.0) as *const () == Arc::as_ptr(&other.0) as *const () }
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
        Ok(Self {
            addr,
        })
    }

    async fn init(&self, node: &Arc<Node<Self>>) {
        node.backend.addr.0.set(node.clone()).ok().unwrap();
    }

    async fn host(node: Arc<Node<Self>>) -> Result<(), Self::Error> {
        let () = futures::future::pending().await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Sender<msg::Greet<Self>> for Mem {
    async fn send(&self, addr: &Self::Addr, msg: msg::Greet<Self>) -> Result<msg::GreetResp<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_greet(msg).await)
    }
}

#[async_trait::async_trait]
impl Sender<msg::Ping<Self>> for Mem {
    async fn send(&self, addr: &Self::Addr, msg: msg::Ping<Self>) -> Result<msg::Pong<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_ping(msg).await)
    }
}

#[async_trait::async_trait]
impl Sender<msg::Discover<Self>> for Mem {
    async fn send(&self, addr: &Self::Addr, msg: msg::Discover<Self>) -> Result<msg::DiscoverResp<Self>, Self::Error> {
        Ok(addr.0.get().unwrap().recv_discover(msg).await)
    }
}
