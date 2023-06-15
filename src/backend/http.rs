use crate::{Node, Backend, Sender, msg::{self, Msg}};

use axum::{
    extract::{Path, State},
    routing::{get, Router},
    Json, Server,
};
use reqwest::Url;
use serde::{Serialize, de::DeserializeOwned};
use std::{net::SocketAddr, sync::Arc};

#[derive(Debug)]
pub enum Error {
    Hyper(hyper::Error),
    Reqwest(reqwest::Error),
}

pub struct Config {
    pub bind_addr: SocketAddr,
}

pub struct Http {
    config: Config,
    client: reqwest::Client,
}

#[async_trait::async_trait]
impl Backend for Http {
    type Addr = String;
    type Config = Config;
    type Error = Error;

    async fn create(config: Self::Config) -> Result<Self, Self::Error> {
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }

    async fn host(node: Arc<Node<Self>>) -> Result<(), Self::Error> {
        let peer_router = Router::new()
            .route("/greet", get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                Json(node.recv_greet(msg).await)
            }))
            .route("/ping", get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                Json(node.recv_ping(msg).await)
            }))
            .route("/discover", get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                Json(node.recv_discover(msg).await)
            }));

        let data_router = Router::new()
            .route("/:hash", get(|node: State<Arc<Node<_>>>, Path(id)| async move {
                node.fetch_data(id).await.map(Json)
            }));

        let router = Router::new()
            .nest("/peer", peer_router)
            .nest("/data", data_router)
            .with_state(node.clone());

        Server::bind(&node.backend.config.bind_addr)
            .serve(router.into_make_service())
            .await
            .map_err(Error::Hyper)
    }
}

#[async_trait::async_trait]
impl Sender<msg::Greet<Self>> for Http {
    async fn send(&self, addr: &Self::Addr, msg: msg::Greet<Self>) -> Result<msg::GreetResp<Self>, Self::Error> {
        self.send_inner("/peer/greet", addr, msg).await
    }
}

#[async_trait::async_trait]
impl Sender<msg::Ping<Self>> for Http {
    async fn send(&self, addr: &Self::Addr, msg: msg::Ping<Self>) -> Result<msg::Pong<Self>, Self::Error> {
        self.send_inner("/peer/ping", addr, msg).await
    }
}

#[async_trait::async_trait]
impl Sender<msg::Discover<Self>> for Http {
    async fn send(&self, addr: &Self::Addr, msg: msg::Discover<Self>) -> Result<msg::DiscoverResp<Self>, Self::Error> {
        self.send_inner("/peer/discover", addr, msg).await
    }
}

impl Http {
    async fn send_inner<M: Msg<Http> + Serialize>(
        &self,
        path: &str,
        addr: &str,
        msg: M,
    ) -> Result<M::Resp, Error> where M::Resp: DeserializeOwned {
        let url = addr
            .parse::<Url>()
            .unwrap()
            .join(path)
            .unwrap();
        self.client
            .get(url)
            .json(&msg)
            .send()
            .await
            .map_err(Error::Reqwest)?
            .json()
            .await
            .map_err(Error::Reqwest)
    }
}
