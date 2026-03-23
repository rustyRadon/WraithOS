use anyhow::{Context, Result};
use dashmap::DashMap;
use futures::{future::{BoxFuture, FutureExt}, SinkExt, StreamExt};
use lru::LruCache;
use mdns_sd::ServiceDaemon;
use sentinel_crypto::NodeIdentity;
use sentinel_protocol::{
    messages::{MessageContent, PeerInfo, SentinelMessage},
    SentinelCodec, SignalingMessage,
};
use sentinel_transport::{SentinelAcceptor, SentinelConnector};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::io::Write;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::codec::Framed;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileManifest {
    pub id: Uuid,
    pub name: String,
    pub total_chunks: u32,
    pub chunks: Vec<String>, // chunk hashes
}

use crate::network::socket::FighterSocket;
use crate::SentinelEvent;

pub struct PeerState {
    pub tx: mpsc::UnboundedSender<SentinelMessage>,
    pub node_id: String,
    pub node_name: String,
    pub public_key: Option<Vec<u8>>,
    pub last_seen: std::time::Instant,
}

pub struct SentinelNode {
    pub identity: NodeIdentity,
    pub listen_port: u16,
    pub public_addr: RwLock<Option<SocketAddr>>,
    pub acceptor: SentinelAcceptor,
    pub db: sled::Db,
    pub mdns: ServiceDaemon,
    pub peers: DashMap<String, PeerState>,
    pub seen_messages: Mutex<LruCache<Uuid, ()>>,
    pub signaler_tx: mpsc::UnboundedSender<SentinelMessage>,
}

impl SentinelNode {
    pub async fn new(data_dir: PathBuf, listen_port: u16) -> Result<(Self, mpsc::UnboundedReceiver<SentinelMessage>)> {
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)?;
        }
        let identity = NodeIdentity::load_or_generate(data_dir.join("identity.key"))?;
        let db = sled::open(data_dir.join("storage.db"))?;

        let cert_path = if data_dir.join("node.crt").exists() {
            data_dir.join("node.crt")
        } else {
            PathBuf::from("certs/server.crt")
        };
        let key_path = if data_dir.join("node.key").exists() {
            data_dir.join("node.key")
        } else {
            PathBuf::from("certs/server.key")
        };

        let acceptor = SentinelAcceptor::new(&cert_path, &key_path, Duration::from_secs(10))?;
        let mdns = ServiceDaemon::new().context("mDNS initialization failed")?;
        let seen_messages = Mutex::new(LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()));

        let (signaler_tx, signaler_rx) = mpsc::unbounded_channel();

        Ok((
            Self {
                identity,
                listen_port,
                public_addr: RwLock::new(None),
                acceptor,
                db,
                mdns,
                peers: DashMap::new(),
                seen_messages,
                signaler_tx,
            },
            signaler_rx,
        ))
    }

    pub async fn run(self: Arc<Self>, event_tx: mpsc::UnboundedSender<SentinelEvent>) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.listen_port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        
        let _ = event_tx.send(SentinelEvent::SystemLog(format!("Engine active on {}", addr)));

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            let node = Arc::clone(&self);
            let tx = event_tx.clone();
            let addr_str = remote_addr.to_string();

            tokio::spawn(async move {
                if let Ok(tls) = node.acceptor.accept(stream).await {
                    let (mut sink, mut stream_in) = Framed::new(tls, SentinelCodec::new()).split();
                    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel();

                    let hs = SentinelMessage::new(
                        node.identity.node_id(),
                        MessageContent::Handshake {
                            public_key: node.identity.public_key_bytes(),
                            node_name: "Sentinel-Core-Node".into(),
                        },
                    );
                    node.sign_and_send(&peer_tx, hs);

                    node.peers.insert(addr_str.clone(), PeerState {
                        tx: peer_tx,
                        node_id: "pending".into(),
                        node_name: "Inbound".into(),
                        public_key: None,
                        last_seen: std::time::Instant::now(),
                    });

                    tokio::spawn(async move {
                        while let Some(msg) = peer_rx.recv().await {
                            if sink.send(msg).await.is_err() { break; }
                        }
                    });

                    while let Some(Ok(msg)) = stream_in.next().await {
                        if let MessageContent::Chat(text) = &msg.content {
                            if text != "PING" {
                                let _ = tx.send(SentinelEvent::ChatMessage {
                                    sender: msg.sender.clone(),
                                    text: text.clone(),
                                });
                            }
                        }
                        let _ = node.clone().handle_incoming_message(msg, addr_str.clone()).await;
                    }
                    node.peers.remove(&addr_str);
                    let _ = tx.send(SentinelEvent::SystemLog(format!("Peer disconnected: {}", addr_str)));
                }
            });
        }
    }

    pub async fn request_ghost_by_id(self: Arc<Self>, target_id: String) -> Result<()> {
        let existing_addr = self.peers.iter().find_map(|entry| {
            if entry.value().node_id == target_id { Some(entry.key().clone()) } else { None }
        });

        let target_addr = if let Some(addr) = existing_addr {
            addr
        } else {
            self.query_registry(&target_id).await?
        };

        self.dial_peer(target_addr).await
    }

    pub async fn dial_peer(self: Arc<Self>, addr: String) -> Result<()> {
        let target_addr: SocketAddr = addr.to_socket_addrs()?.next()
            .context("Address resolution failed")?;

        if target_addr.port() == self.listen_port || self.peers.contains_key(&addr) {
            return Ok(());
        }

        let local_bind = SocketAddr::from(([0, 0, 0, 0], self.listen_port));
        let fighter = FighterSocket::create_war_ready(local_bind)
            .or_else(|_| FighterSocket::create_war_ready(SocketAddr::from(([0, 0, 0, 0], 0))))?;

        let _ = fighter.connect(&target_addr.into());
        
        let std_stream: std::net::TcpStream = fighter.into();
        let tokio_stream = TokioTcpStream::from_std(std_stream)?;
        tokio_stream.writable().await?;

        let connector = SentinelConnector::new();
        let tls = connector.connect("sentinel-node.local", tokio_stream).await?;

        let (mut sink, mut stream) = Framed::new(tls, SentinelCodec::new()).split();
        let (tx, mut rx) = mpsc::unbounded_channel();

        self.peers.insert(addr.clone(), PeerState {
            tx: tx.clone(),
            node_id: "pending".into(),
            node_name: "Ghost".into(),
            public_key: None,
            last_seen: std::time::Instant::now(),
        });

        let hs = SentinelMessage::new(self.identity.node_id(), MessageContent::Handshake {
            public_key: self.identity.public_key_bytes(),
            node_name: "Sentinel-Node".into(),
        });
        self.sign_and_send(&tx, hs);

        let addr_io = addr.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if sink.send(msg).await.is_err() { break; }
            }
        });

        let node_inner = Arc::clone(&self);
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                let _ = node_inner.clone().handle_incoming_message(msg, addr_io.clone()).await;
            }
            node_inner.peers.remove(&addr_io);
        });

        Ok(())
    }

    pub async fn query_registry(&self, target_id: &str) -> Result<String> {
        let signaler_addr = "127.0.0.1:8888"; // Update this to your deployed Signaler IP
        
        let stream = TokioTcpStream::connect(signaler_addr).await
            .context("Signaler unreachable")?;
        let mut framed = Framed::new(stream, SentinelCodec::new());

        let reg = SentinelMessage::new(
            self.identity.node_id(),
            MessageContent::Signal(SignalingMessage::Register {
                node_id: self.identity.node_id(),
                public_key: self.identity.public_key_bytes(),
                signature: vec![], 
            }),
        );
        framed.send(reg).await?;

        let lookup = SentinelMessage::new(
            self.identity.node_id(),
            MessageContent::Signal(SignalingMessage::LookupRequest { 
                target_id: target_id.to_string() 
            }),
        );
        framed.send(lookup).await?;

        while let Some(Ok(resp)) = framed.next().await {
            if let MessageContent::Signal(SignalingMessage::PeerResponse { public_addr, .. }) = resp.content {
                return Ok(public_addr.to_string());
            }
            if let MessageContent::Signal(SignalingMessage::Error(e)) = resp.content {
                return Err(anyhow::anyhow!("Signaler Error: {}", e));
            }
        }
        
        Err(anyhow::anyhow!("Ghost ID {} not found in local or global registries.", target_id))
    }

    pub async fn start_signaler_client(
        self: Arc<Self>, 
        signaler_addr: String, 
        mut signaler_outbound: mpsc::UnboundedReceiver<SentinelMessage>
    ) {
        loop {
            if let Ok(stream) = tokio::net::TcpStream::connect(&signaler_addr).await {
                let mut framed = Framed::new(stream, SentinelCodec::new());
                let my_id = self.identity.node_id();
                
                let reg = SentinelMessage::new_signal(my_id.clone(), SignalingMessage::Register {
                    node_id: my_id,
                    public_key: self.identity.public_key_bytes(),
                    signature: vec![], 
                });

                if framed.send(reg).await.is_ok() {
                    let (mut sink, mut stream) = framed.split();
                    loop {
                        tokio::select! {
                            Some(out_msg) = signaler_outbound.recv() => {
                                if sink.send(out_msg).await.is_err() { break; }
                            }
                            Some(Ok(msg)) = stream.next() => {
                                if let MessageContent::Signal(SignalingMessage::PeerResponse { public_addr, .. }) = msg.content {
                                    let node = Arc::clone(&self);
                                    tokio::spawn(async move { let _ = node.dial_peer(public_addr.to_string()).await; });
                                }
                            }
                            else => break,
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    pub(crate) fn handle_incoming_message(self: Arc<Self>, msg: SentinelMessage, addr: String) -> BoxFuture<'static, Result<()>> {
        let node = self.clone();
        async move {
            if let Some(mut peer) = node.peers.get_mut(&addr) {
                peer.last_seen = std::time::Instant::now();
            }

            {
                let mut seen = node.seen_messages.lock().await;
                if seen.contains(&msg.id) { return Ok(()); }
                seen.put(msg.id, ());
            }

            if !msg.signature.is_empty() && !NodeIdentity::verify(&msg.sig_hash(), &msg.signature, &msg.public_key) {
                return Err(anyhow::anyhow!("Forged signature detected from {}", addr));
            }

            match &msg.content { 
                MessageContent::Handshake { public_key, node_name } => {
                    if let Some(mut peer) = node.peers.get_mut(&addr) {
                        peer.node_id = msg.sender.clone();
                        peer.node_name = node_name.clone(); 
                        peer.public_key = Some(public_key.clone()); 

                        println!("\n[!] Incoming connection from Ghost: {}", peer.node_id);
                        println!("    Name: {} | Trust: Level 1 (Handshaked)", peer.node_name);
                        print!("\nwraith@{} >> ", &node.identity.node_id()[..8]);
                        let _ = std::io::stdout().flush();                        
                    }
                }

                MessageContent::Chat(text) if text != "PING" => {
                    let _ = node.persist_message(&msg);
                }

                MessageContent::DirectoryRequest => {
                    let library = node.get_local_library()?;
                    let response = SentinelMessage::new(
                        node.identity.node_id(),
                        MessageContent::DirectoryResponse(library)
                    );
                    if let Some(peer) = node.peers.get(&addr) {
                        node.sign_and_send(&peer.tx, response);
                    }
                }

                MessageContent::DirectoryResponse(remote_library) => {
                    println!("\n[!] Received Library from {}:", addr);
                    if remote_library.is_empty() {
                        println!("    [The ghost has no manifested files]");
                    }
                    for file in remote_library {
                        println!("  > {} ({} chunks) | ID: {}", file.name, file.total_chunks, file.id);
                    }
                    print!("\nwraith@{} >> ", &node.identity.node_id()[..8]);
                    let _ = std::io::stdout().flush();
                }

                MessageContent::DataRequest { file_id, chunk_index } => {
                    if let Ok(Some(chunk_data)) = node.db.open_tree("file_chunks")?
                        .get(format!("{}_{}", file_id, chunk_index)) 
                    {
                        let response = SentinelMessage::new(
                            node.identity.node_id(),
                            MessageContent::DataResponse {
                                file_id: *file_id,
                                chunk_index: *chunk_index,
                                data: chunk_data.to_vec(),
                            }
                        );
                        if let Some(peer) = node.peers.get(&addr) {
                            node.sign_and_send(&peer.tx, response);
                        }
                    }
                }

                _ => {}
            }
            Ok(())
        }.boxed()
    }

    pub async fn is_local_peer(&self, target: SocketAddr) -> bool {
        if let Some(my_public) = *self.public_addr.read().await {
            return target.ip() == my_public.ip();
        }
        false
    }

    pub async fn discover_and_set_public_ip(&self) -> Result<()> {
        match FighterSocket::discover_public_ip(self.listen_port).await {
            Ok(addr) => {
                let mut lock = self.public_addr.write().await;
                *lock = Some(addr);
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("STUN discovery failed: {}", e)),
        }
    }

    pub fn sign_and_send(&self, tx: &mpsc::UnboundedSender<SentinelMessage>, mut msg: SentinelMessage) {
        msg.public_key = self.identity.public_key_bytes();
        msg.signature = self.identity.sign(&msg.sig_hash());
        let _ = tx.send(msg);
    }

    pub async fn start_heartbeat_service(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(20));
        loop {
            interval.tick().await;
            let ping = SentinelMessage::new(self.identity.node_id(), MessageContent::Chat("PING".into()));
            for entry in self.peers.iter() {
                self.sign_and_send(&entry.value().tx, ping.clone());
            }
            self.peers.retain(|_, state| state.last_seen.elapsed() < Duration::from_secs(60));
        }
    }

    pub async fn start_gossip_service(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let peer_list: Vec<PeerInfo> = self.peers.iter().filter_map(|e| {
                e.key().parse().ok().map(|addr| PeerInfo {
                    node_id: e.value().node_id.clone(),
                    address: addr,
                    node_name: e.value().node_name.clone(),
                    last_seen: 0,
                })
            }).collect();
            
            if !peer_list.is_empty() {
                let msg = SentinelMessage::new(self.identity.node_id(), MessageContent::PeerDiscovery(peer_list));
                for entry in self.peers.iter() { self.sign_and_send(&entry.value().tx, msg.clone()); }
            }
        }
    }

    pub fn ingest_file(&self, name: &str, file_id: Uuid, chunks: Vec<Vec<u8>>) -> Result<()> {
        let metadata_tree = self.db.open_tree("file_metadata")?;
        let chunks_tree = self.db.open_tree("file_chunks")?;
        for (i, data) in chunks.iter().enumerate() {
            chunks_tree.insert(format!("{}_{}", file_id, i), data.as_slice())?;
        }
        let meta = sentinel_protocol::messages::FileMetadata {
            id: file_id,
            name: name.to_string(),
            size: (chunks.len() * 1024 * 1024) as u64,
            total_chunks: chunks.len() as u32,
            merkle_root: vec![],
        };
        metadata_tree.insert(file_id.as_bytes(), bincode::serialize(&meta)?)?;
        Ok(())
    }

    pub fn get_local_library(&self) -> Result<Vec<sentinel_protocol::messages::FileMetadata>> {
        let metadata_tree = self.db.open_tree("file_metadata")?;
        let mut library = Vec::new();
        for item in metadata_tree.iter() {
            let (_, v) = item?;
            let meta: sentinel_protocol::messages::FileMetadata = bincode::deserialize(&v)?;
            library.push(meta);
        }
        Ok(library)
    }

    pub async fn request_peer_library(&self, peer_addr: &str) -> Result<()> {
        let msg = SentinelMessage::new(self.identity.node_id(), MessageContent::DirectoryRequest);
        if let Some(peer) = self.peers.get(peer_addr) {
            self.sign_and_send(&peer.tx, msg);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Peer disconnected"))
        }
    }

    pub async fn request_file_chunk(&self, peer_addr: &str, file_id: Uuid, chunk_index: u32) -> Result<()> {
        let msg = SentinelMessage::new(self.identity.node_id(), MessageContent::DataRequest { file_id, chunk_index });
        if let Some(peer) = self.peers.get(peer_addr) {
            self.sign_and_send(&peer.tx, msg);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Peer not found"))
        }
    }

    pub fn persist_message(&self, msg: &SentinelMessage) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        tree.insert(format!("{}:{}", msg.timestamp, msg.sender), msg.to_bytes())?;
        Ok(())
    }

    pub fn print_history(&self) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        for item in tree.iter().values().rev().take(10) { 
            let item = item?;
            if let Ok(msg) = SentinelMessage::from_bytes(&item) {
                if let MessageContent::Chat(text) = msg.content {
                    println!("[{}] {}", msg.sender, text);
                }
            }
        }
        Ok(())
    }

    pub fn get_file_manifest(&self, file_id: &str) -> Result<FileManifest> {
        let metadata_tree = self.db.open_tree("file_metadata")?;
        let file_uuid = Uuid::parse_str(file_id)?;
        let meta_bytes = metadata_tree.get(file_uuid.as_bytes())?
            .ok_or_else(|| anyhow::anyhow!("File not found"))?;
        let meta: sentinel_protocol::messages::FileMetadata = bincode::deserialize(&meta_bytes)?;
        
        let chunks_tree = self.db.open_tree("file_chunks")?;
        let mut chunks = Vec::new();
        for i in 0..meta.total_chunks {
            let key = format!("{}_{}", file_uuid, i);
            if chunks_tree.get(&key)?.is_some() {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                chunks.push(format!("{:x}", hasher.finish()));
            }
        }
        
        Ok(FileManifest {
            id: meta.id,
            name: meta.name,
            total_chunks: meta.total_chunks,
            chunks,
        })
    }

    pub async fn load_chunk(&self, chunk_hash: &str) -> Result<Vec<u8>> {
        let chunks_tree = self.db.open_tree("file_chunks")?;
        for item in chunks_tree.iter() {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            key_str.hash(&mut hasher);
            if format!("{:x}", hasher.finish()) == chunk_hash {
                return Ok(value.to_vec());
            }
        }
        Err(anyhow::anyhow!("Chunk not found"))
    }
}