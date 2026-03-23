pub mod tcp;
pub mod tls;
pub mod tls_config;
pub mod acceptor;
pub mod error;
pub mod metrics;
pub mod state;
pub mod connector;

pub use acceptor::SentinelAcceptor;
pub use error::{TransportError, TransportResult};
pub use tcp::RawTcpTransport;
pub use tls::TlsTransport;
pub use state::{Connection, Unauthenticated};
pub use connector::SentinelConnector;

use tokio::io::{AsyncRead, AsyncWrite};
use std::net::SocketAddr;

pub trait SentinelTransport: AsyncRead + AsyncWrite + Unpin + Send {
    fn peer_addr(&self) -> Result<SocketAddr, std::io::Error>;

    fn is_secure(&self) -> bool;
}