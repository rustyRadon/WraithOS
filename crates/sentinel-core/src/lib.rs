pub mod engine;
pub mod discovery;
pub mod network;

pub use engine::{SentinelNode, PeerState};

#[derive(Debug, Clone)]
pub enum SentinelEvent {
    PeerConnected { peer_id: String, addr: String },
    PeerDisconnected { peer_id: String },
    ChatMessage { sender: String, text: String },
    SystemLog(String),
}