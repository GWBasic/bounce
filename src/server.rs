use async_std::net::{IpAddr, Ipv4Addr, TcpListener, SocketAddr};
use async_std::prelude::*;
use std::io::Error;

use crate::auth::authenticate;
use crate::bridge::run_bridge;
use crate::keys::Key;

pub async fn run_server(port: u16, adapter_port: u16, key: Key) -> Result<(), Error> {

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = TcpListener::bind(socket_addr).await?;

    let adapter_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), adapter_port);
    let adapter_listener = TcpListener::bind(adapter_socket_addr).await?;

    let mut incoming = listener.incoming();
    let mut adapter_incoming = adapter_listener.incoming();

    log::info!("Bounce server: Listening for incoming connections on {}, accepting adapter on port {}", port, adapter_port);
    
    'adapter_accept:
    while let Some(adapter_stream) = adapter_incoming.next().await {
        match adapter_stream {
            // TODO: Return errors
            Err(err) => log::error!("Error accepting adapter stream: {}", err),
            Ok(mut adapter_stream) => {

                log::info!("Incoming adapter stream: {:?}", adapter_stream.peer_addr().unwrap());

                let xors = match authenticate(key.clone(), adapter_stream.clone()).await {
                    Err(err) => {
                        log::error!("Bad client: {}", err);
                        continue 'adapter_accept;
                    },
                    Ok(n) => n
                };

                let stream;
                
                'accept: loop {
                    match incoming.next().await {
                        Some(s) => {
                            match s {
                                Err(err) => log::error!("Error accepting incoming stream: {}", err),
                                Ok(s) => {
                                    stream = s;
                                    break 'accept;
                                }
                            }
                        }
                        None => {},
                    }
                }

                log::info!("Incoming clear stream: {:?}", stream.peer_addr().unwrap());

                match adapter_stream.write_all(b"connected").await {
                    Err(err) => {
                        log::error!("Error starting connection: {}", err);
                        continue 'adapter_accept;
                    },
                    Ok(()) => {}
                }

                run_bridge(xors, stream, "incoming".to_string(), adapter_stream, "bounce-outgoing".to_string());
            }
        }
    }

    Ok(())
}

