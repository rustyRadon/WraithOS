// yo, this is for finding other nodes on the local network using mdns
// kinda like shouting "hey i'm here" and listening for others
use anyhow::Result;
use std::sync::Arc;
use crate::engine::SentinelNode;
use mdns_sd::{ServiceInfo, ServiceEvent};

// starts the discovery thing, basically sets up mdns to find buddies nearby
pub async fn start_discovery(node: Arc<SentinelNode>, port: u16) -> Result<()> {
    // service type for our sentinel nodes, local only
    let service_type = "_sentinel._tcp.local.";
    // make a unique name using part of our node id
    let instance_name = format!("node-{}", &node.identity.node_id()[..8]);
    
    // create our service info to advertise ourselves
    let my_info = ServiceInfo::new(
        service_type,
        &instance_name,
        &format!("{}.local.", instance_name),
        "127.0.0.1", 
        port,
        None,
    )?;
    
    // register ourselves so others can find us
    node.mdns.register(my_info)?;

    // start browsing for other services
    let receiver = node.mdns.browse(service_type)?;
    
    // spawn a task to listen for discovery events
    tokio::spawn(async move {
        // loop forever listening for events
        while let Ok(event) = receiver.recv_async().await {
            // if we found a service, try to connect
            if let ServiceEvent::ServiceResolved(info) = event {
                // get the first address
                let addr = info.get_addresses().iter().next();
                if let Some(ip) = addr {
                    // build the full address string
                    let full_addr = format!("{}:{}", ip, info.get_port());
                    
                    // check if we're not already connected and not localhost
                    if !node.peers.contains_key(&full_addr) && ip.to_string() != "0.0.0.0" {
                        // clone the node for the new task
                        let n = Arc::clone(&node);
                        // spawn a task to dial the peer
                        tokio::spawn(async move {
                            // try to connect, ignore errors for now
                            if let Err(e) = n.dial_peer(full_addr).await {
                                let _ = e; 
                            }
                        });
                    }
                }
            }
        }
    });

    // all good
    Ok(())
}