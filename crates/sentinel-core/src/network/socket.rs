use socket2::{Socket, Domain, Type, Protocol, SockAddr};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket as StdUdpSocket};
use anyhow::{Result, anyhow};
use stunclient::StunClient; 

pub struct FighterSocket;

impl FighterSocket {
    /// Discovers public IP using a blocking task to avoid Future 0.1 compatibility issues
    pub async fn discover_public_ip(local_port: u16) -> Result<SocketAddr> {
        let local_udp_addr: SocketAddr = format!("0.0.0.0:{}", local_port).parse()?;
        let udp_socket = StdUdpSocket::bind(local_udp_addr)?;

        let stun_server = "stun.l.google.com:19302"
            .to_socket_addrs()?
            .find(|x| x.is_ipv4())
            .ok_or_else(|| anyhow!("Failed to resolve STUN server"))?;

        let client = StunClient::new(stun_server);
        
        let public_addr = tokio::task::spawn_blocking(move || {
            client.query_external_address(&udp_socket)
                .map_err(|e| e.to_string())
        })
        .await? 
        .map_err(|e| anyhow!("STUN query failed: {}", e))?; 

        Ok(public_addr)
    }

    /// Creates a socket configured for TCP Simultaneous Open (Hole Punching)
    pub fn create_war_ready(local_addr: SocketAddr) -> Result<Socket> {
        let domain = if local_addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

        //  allow multiple sockets to bind to the same port maywheather the face with seies of punch lol
        socket.set_reuse_address(true)?;
        
        #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
        socket.set_reuse_address(true)?;

        socket.bind(&SockAddr::from(local_addr))?;
        socket.set_nonblocking(true)?;

        Ok(socket)
    }
}