use socket2::{Socket, Domain, Type, Protocol, SockAddr};
use std::net::{SocketAddr, /*ToSocketAddrs,*/ UdpSocket as StdUdpSocket};
use anyhow::{Result, anyhow};

pub struct FighterSocket;

impl FighterSocket {
    pub async fn discover_public_ip(local_port: u16) -> Result<SocketAddr> {
        let stun_server = "stun.l.google.com:19302";
        let local_udp_addr: SocketAddr = format!("0.0.0.0:{}", local_port).parse()?;
        
        let socket = StdUdpSocket::bind(local_udp_addr)?;
        socket.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;

        let mut request = [0u8; 20];
        request[0..2].copy_from_slice(&[0x00, 0x01]);
        request[4..8].copy_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
        request[8..20].copy_from_slice(&[0x13, 0x37, 0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x02]);

        let result = tokio::task::spawn_blocking(move || {
            socket.send_to(&request, stun_server)?;
            
            let mut buf = [0u8; 512];
            let (len, _) = socket.recv_from(&mut buf)?;
            
            if len < 20 { return Err(anyhow!("Short STUN response")); }

            for i in 20..len-4 {
                if buf[i..i+2] == [0x00, 0x20] { // XOR-MAPPED-ADDRESS attribute
                    let port = (u16::from_be_bytes([buf[i+6], buf[i+7]])) ^ 0x2112;
                    let ip = [
                        buf[i+8] ^ 0x21, buf[i+9] ^ 0x12, 
                        buf[i+10] ^ 0xA4, buf[i+11] ^ 0x42
                    ];
                    return Ok(SocketAddr::new(ip.into(), port));
                }
            }
            Err(anyhow!("No XOR-MAPPED-ADDRESS found in STUN response"))
        }).await;

        match result {
            Ok(Ok(addr)) => Ok(addr),
            _ => {
                eprintln!("\n[!] Manual STUN failed. Operating in Local Sector mode.");
                Ok(local_udp_addr)
            }
        }
    }

    pub fn create_war_ready(local_addr: SocketAddr) -> Result<Socket> {
        let domain = if local_addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

        socket.set_reuse_address(true)?;
        
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            let optval: libc::c_int = 1;
            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEPORT,
                    &optval as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) != 0 {
                    eprintln!("[!] Warning: Kernel rejected SO_REUSEPORT");
                }
            }
        }

        socket.bind(&SockAddr::from(local_addr))?;
        socket.set_nonblocking(true)?;

        Ok(socket)
    }
}