use crate::{Backend, Node, PublicId, Tag};

use axum::{
    body::Bytes,
    extract::{Path, State},
    routing::{get, Router},
    Json, Server,
};
use hyper::StatusCode;
use reqwest::Url;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("hyper: {0}")]
    Hyper(hyper::Error),
    #[error("reqwest: {0}")]
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
                get(|node: State<Arc<Node<_>>>, msg: Json<Locate>| async move {
                    Json(LocateResp {
                        result: node.recv_locate(msg.tag).await,
                    })
                }),
            )
            .route(
                "/upload",
                get(|node: State<Arc<Node<_>>>, msg: Json<Upload>| async move {
                    Json(UploadResp {
                        result: node.recv_upload(msg.0.data).await,
                    })
                }),
            )
            .route(
                "/download",
                get(
                    |node: State<Arc<Node<_>>>, msg: Json<Download>| async move {
                        Json(DownloadResp {
                            data: node.recv_download(msg.tag).await,
                        })
                    },
                ),
            );

        let data_router = Router::new()
            .route(
                "/:hash",
                get(|node: State<Arc<Node<_>>>, Path(id)| async move {
                    match Tag::try_from_hex::<String>(id) {
                        Ok(tag) => match node.do_download(tag).await {
                            Ok(Some(data)) => (StatusCode::OK, Ok(Bytes::from(data))),
                            Ok(None) => (StatusCode::NOT_FOUND, Err("data does not exist")),
                            Err(err) => (StatusCode::BAD_GATEWAY, Err(err)),
                        },
                        Err(err) => (StatusCode::BAD_REQUEST, Err(err)),
                    }
                }),
            )
            .route(
                "/upload",
                get(|node: State<Arc<Node<_>>>, bytes: Bytes| async move {
                    match node.do_upload(bytes.to_vec().into_boxed_slice()).await {
                        Ok(tag) => (StatusCode::CREATED, tag.to_string()),
                        Err(err) => (StatusCode::BAD_GATEWAY, err.into()),
                    }
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
        tag: Tag,
    ) -> Result<Result<bool, (PublicId, Self::Addr)>, Self::Error> {
        Ok(self
            .send_inner("/peer/locate", addr, Locate { tag })
            .await?
            .result)
    }

    async fn send_upload(
        &self,
        addr: &Self::Addr,
        data: Box<[u8]>,
    ) -> Result<Result<(), ()>, Self::Error> {
        Ok(self
            .send_inner("/peer/upload", addr, Upload { data })
            .await?
            .result)
    }

    async fn send_download(
        &self,
        addr: &Self::Addr,
        tag: Tag,
    ) -> Result<Option<Box<[u8]>>, Self::Error> {
        Ok(self
            .send_inner("/peer/download", addr, Download { tag })
            .await?
            .data)
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
struct Greet {
    sender: (PublicId, String),
}

#[derive(Serialize, Deserialize)]
struct GreetResp {
    result: Result<PublicId, Option<String>>,
}

impl Msg for Greet {
    type Resp = GreetResp;
}

#[derive(Serialize, Deserialize)]
struct Ping;

#[derive(Serialize, Deserialize)]
struct Pong;

impl Msg for Ping {
    type Resp = Pong;
}

/// Attempt to discover a new peer by asking existing peers.
///
/// `addr` specifies the original requesting peer.
/// `max_level` specifies the maximum distance (log2) that the returned peer should be from the given address
#[derive(Serialize, Deserialize)]
struct Discover {
    target: Tag,
    max_level: u16,
}

#[derive(Serialize, Deserialize)]
struct DiscoverResp {
    peer: Option<(PublicId, String)>,
}

impl Msg for Discover {
    type Resp = DiscoverResp;
}

/// Attempt to discover a tag in the network.
#[derive(Serialize, Deserialize)]
struct Locate {
    tag: Tag,
}

#[derive(Serialize, Deserialize)]
struct LocateResp {
    // Ok(true) => I own the resource
    // Ok(false) => I do not own the resource and do not know anybody closer to the resource (404!)
    // Err(_) => I do not own the resource but this other node is closer to it
    pub result: Result<bool, (PublicId, String)>,
}

impl Msg for Locate {
    type Resp = LocateResp;
}

#[derive(Serialize, Deserialize)]
struct Upload {
    data: Box<[u8]>,
}

#[derive(Serialize, Deserialize)]
struct UploadResp {
    result: Result<(), ()>,
}

impl Msg for Upload {
    type Resp = UploadResp;
}

#[derive(Serialize, Deserialize)]
struct Download {
    pub tag: Tag,
}

#[derive(Serialize, Deserialize)]
struct DownloadResp {
    // Some(_) => I own the resource and here it is
    // None => I do not own the resource
    pub data: Option<Box<[u8]>>,
}

impl Msg for Download {
    type Resp = DownloadResp;
}
