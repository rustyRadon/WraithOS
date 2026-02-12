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
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::codec::Framed;
use uuid::Uuid;

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
    /// Initializes a new SentinelNode instance with persistent storage and identity
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

    /// The Main Engine Loop: Handles the TCP Listener and translates network bytes into SentinelEvents
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

                    // 1. Handshake setup
                    let hs = SentinelMessage::new(
                        node.identity.node_id(),
                        MessageContent::Handshake {
                            public_key: node.identity.public_key_bytes(),
                            node_name: "Sentinel-Core-Node".into(),
                        },
                    );
                    node.sign_and_send(&peer_tx, hs);

                    // 2. Register Peer internally
                    node.peers.insert(addr_str.clone(), PeerState {
                        tx: peer_tx,
                        node_id: "pending".into(),
                        node_name: "Inbound".into(),
                        public_key: None,
                        last_seen: std::time::Instant::now(),
                    });

                    // 3. Outbound Worker (Library Internal)
                    tokio::spawn(async move {
                        while let Some(msg) = peer_rx.recv().await {
                            if sink.send(msg).await.is_err() { break; }
                        }
                    });

                    // 4. Inbound Message Loop
                    while let Some(Ok(msg)) = stream_in.next().await {
                        // Emit high-level event for UI
                        if let MessageContent::Chat(text) = &msg.content {
                            if text != "PING" {
                                let _ = tx.send(SentinelEvent::ChatMessage {
                                    sender: msg.sender.clone(),
                                    text: text.clone(),
                                });
                            }
                        }
                        
                        // Process protocol logic
                        let _ = node.clone().handle_incoming_message(msg, addr_str.clone()).await;
                    }
                    node.peers.remove(&addr_str);
                    let _ = tx.send(SentinelEvent::SystemLog(format!("Peer disconnected: {}", addr_str)));
                }
            });
        }
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

    pub async fn dial_peer(self: Arc<Self>, addr: String) -> Result<()> {
        let target_addr: SocketAddr = addr.to_socket_addrs()?.next().context("Address resolution failed")?;

        if target_addr.port() == self.listen_port || self.peers.contains_key(&addr) {
            return Ok(());
        }

        let local_bind = SocketAddr::from(([0, 0, 0, 0], self.listen_port));
        let fighter = FighterSocket::create_war_ready(local_bind)
            .or_else(|_| FighterSocket::create_war_ready(SocketAddr::from(([0, 0, 0, 0], 0))))?;

        match fighter.connect(&target_addr.into()) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.raw_os_error() == Some(115) => {}
            Err(e) => return Err(anyhow::anyhow!("Fighter punch failed: {}", e)),
        }

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
            node_name: "Outbound".into(),
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

    pub fn print_history(&self) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        for item in tree.iter().values().rev().take(10) { let item = item?;
            if let Ok(msg) = SentinelMessage::from_bytes(&item) {
                if let MessageContent::Chat(text) = msg.content {
                    println!("[{}] {}", msg.sender, text);
                }
            }
        }
        Ok(())
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
                return Ok(());
            }

            match &msg.content { 
                MessageContent::Handshake { public_key, node_name } => {
                    if let Some(mut peer) = node.peers.get_mut(&addr) {
                        peer.node_id = msg.sender.clone();
                        peer.node_name = node_name.clone(); 
                        peer.public_key = Some(public_key.clone()); 
                    }
                }
                MessageContent::Chat(text) if text != "PING" => {
                    let _ = node.persist_message(&msg);
                }
                _ => {}
            }
            Ok(())
        }.boxed()
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

    pub fn persist_message(&self, msg: &SentinelMessage) -> Result<()> {
        let tree = self.db.open_tree("messages")?;
        tree.insert(format!("{}:{}", msg.timestamp, msg.sender), msg.to_bytes())?;
        Ok(())
    }
}