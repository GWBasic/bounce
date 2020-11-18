use async_std::net::{IpAddr, Ipv4Addr, TcpListener, SocketAddr};
use async_std::prelude::*;

use crate::auth::authenticate;
use crate::bridge::bridge;
use crate::keys::Key;

pub async fn run_server(port: u16, adapter_port: u16, key: Key) {

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = TcpListener::bind(socket_addr).await.unwrap();

    let adapter_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), adapter_port);
    let adapter_listener = TcpListener::bind(adapter_socket_addr).await.unwrap();

    let mut incoming = listener.incoming();
    let mut adapter_incoming = adapter_listener.incoming();

    println!("Bounce server: Listening for incoming connections on {}, accepting adapter on port {}", port, adapter_port);
    
    'adapter_accept:
    while let Some(adapter_stream) = adapter_incoming.next().await {
        match adapter_stream {
            Err(err) => println!("Error accepting adapter stream: {}", err),
            Ok(mut adapter_stream) => {

                // TODO: Break this out into a sub-function
                // If there is an error, shutdown adapter_stream

                // TODO task::spawn

                println!("Incoming adapter stream");

                match authenticate(key.clone(), adapter_stream.clone()).await {
                    Err(err) => {
                        println!("Bad client: {}", err);
                        continue 'adapter_accept;
                    },
                    _ => {}
                };

                let stream;
                
                'accept: loop {
                    match incoming.next().await {
                        Some(s) => {
                            match s {
                                Err(err) => println!("Error accepting incoming stream: {}", err),
                                Ok(s) => {
                                    stream = s;
                                    break 'accept;
                                }
                            }
                        }
                        None => {},
                    }
                }

                match adapter_stream.write_all(b"connected").await {
                    Err(err) => {
                        println!("Error starting connection: {}", err);
                        continue 'adapter_accept;
                    },
                    Ok(()) => {}
                }

                bridge(stream, "incoming".to_string(), adapter_stream, "bounce-outgoing".to_string());
            }
        }
    }
}

