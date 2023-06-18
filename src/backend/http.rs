use crate::{msg, Backend, Node, PublicId, Tag};

use axum::{
    body::Bytes,
    extract::{Path, State},
    routing::{get, Router},
    Json, Server,
};
use reqwest::Url;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

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
            .route(
                "/greet",
                get(|node: State<Arc<Node<_>>>, msg: Json<Greet>| async move {
                    Json(GreetResp {
                        result: node.recv_greet(msg.0.sender).await,
                    })
                }),
            )
            .route(
                "/ping",
                get(|node: State<Arc<Node<_>>>, _: Json<Ping>| async move {
                    node.recv_ping().await;
                    Json(Pong)
                }),
            )
            .route(
                "/discover",
                get(
                    |node: State<Arc<Node<_>>>, msg: Json<Discover>| async move {
                        Json(DiscoverResp {
                            peer: node.recv_discover(msg.target, msg.max_level).await,
                        })
                    },
                ),
            )
            .route(
                "/locate",
                get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                    Json(node.recv_locate(msg).await)
                }),
            )
            .route(
                "/upload",
                get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                    Json(node.recv_upload(msg).await)
                }),
            )
            .route(
                "/download",
                get(|node: State<Arc<Node<_>>>, Json(msg)| async move {
                    Json(node.recv_download(msg).await)
                }),
            );

        let data_router = Router::new()
            .route(
                "/:hash",
                get(|node: State<Arc<Node<_>>>, Path(id)| async move {
                    match Tag::try_from_hex::<String>(id) {
                        Ok(tag) => match node.fetch_data(tag).await {
                            Ok(Some(data)) => Ok(Bytes::from(data)),
                            Ok(None) => Err("data does not exist"),
                            Err(()) => Err("unknown error"),
                        },
                        Err(err) => Err(err),
                    }
                }),
            )
            .route(
                "/upload",
                get(|node: State<Arc<Node<_>>>, bytes: Bytes| async move {
                    Json(
                        node.recv_upload(msg::Upload {
                            data: bytes.to_vec().into_boxed_slice(),
                            phantom: Default::default(),
                        })
                        .await
                        .result
                        .ok(),
                    )
                }),
            );

        let router = Router::new()
            .nest("/peer", peer_router)
            .nest("/data", data_router)
            .with_state(node.clone());

        eprintln!("Starting HTTP server on {}", node.backend.config.bind_addr);

        Server::bind(&node.backend.config.bind_addr)
            .serve(router.into_make_service())
            .await
            .map_err(Error::Hyper)
    }

    async fn send_greet(
        &self,
        addr: &Self::Addr,
        sender: (PublicId, Self::Addr),
    ) -> Result<Result<PublicId, Option<Self::Addr>>, Self::Error> {
        Ok(self
            .send_inner("/peer/greet", addr, Greet { sender })
            .await?
            .result)
    }

    async fn send_ping(&self, addr: &Self::Addr) -> Result<Duration, Self::Error> {
        let now = Instant::now();
        self.send_inner("/peer/ping", addr, Ping).await?;
        Ok(now.elapsed())
    }

    async fn send_discover(
        &self,
        addr: &Self::Addr,
        target: Tag,
        max_level: u16,
    ) -> Result<Option<(PublicId, Self::Addr)>, Self::Error> {
        Ok(self
            .send_inner("/peer/discover", addr, Discover { target, max_level })
            .await?
            .peer)
    }

    async fn send_locate(
        &self,
        addr: &Self::Addr,
        msg: msg::Locate<Self>,
    ) -> Result<msg::LocateResp<Self>, Self::Error> {
        self.send_inner("/peer/locate", addr, msg).await
    }

    async fn send_upload(
        &self,
        addr: &Self::Addr,
        msg: msg::Upload<Self>,
    ) -> Result<msg::UploadResp<Self>, Self::Error> {
        self.send_inner("/peer/upload", addr, msg).await
    }

    async fn send_download(
        &self,
        addr: &Self::Addr,
        msg: msg::Download<Self>,
    ) -> Result<msg::DownloadResp<Self>, Self::Error> {
        self.send_inner("/peer/download", addr, msg).await
    }
}

impl Http {
    async fn send_inner<M: Msg + Serialize>(
        &self,
        path: &str,
        addr: &str,
        msg: M,
    ) -> Result<M::Resp, Error> {
        let url = addr.parse::<Url>().unwrap().join(path).unwrap();
        self.client
            .get(url)
            .timeout(std::time::Duration::from_secs(1))
            .json(&msg)
            .send()
            .await
            .map_err(Error::Reqwest)?
            .json()
            .await
            .map_err(Error::Reqwest)
    }
}

pub trait Msg {
    type Resp: DeserializeOwned;
}

#[derive(Serialize, Deserialize)]
pub struct Greet {
    pub sender: (PublicId, String),
}

#[derive(Serialize, Deserialize)]
pub struct GreetResp {
    pub result: Result<PublicId, Option<String>>,
}

impl Msg for Greet {
    type Resp = GreetResp;
}

#[derive(Serialize, Deserialize)]
pub struct Ping;

#[derive(Serialize, Deserialize)]
pub struct Pong;

impl Msg for Ping {
    type Resp = Pong;
}

/// Attempt to discover a new peer by asking existing peers.
///
/// `addr` specifies the original requesting peer.
/// `max_level` specifies the maximum distance (log2) that the returned peer should be from the given address
#[derive(Serialize, Deserialize)]
pub struct Discover {
    pub target: Tag,
    pub max_level: u16,
}

#[derive(Serialize, Deserialize)]
pub struct DiscoverResp {
    pub peer: Option<(PublicId, String)>,
}

impl Msg for Discover {
    type Resp = DiscoverResp;
}

impl Msg for msg::Locate<Http> {
    type Resp = msg::LocateResp<Http>;
}

impl Msg for msg::Upload<Http> {
    type Resp = msg::UploadResp<Http>;
}

impl Msg for msg::Download<Http> {
    type Resp = msg::DownloadResp<Http>;
}
