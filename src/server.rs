use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream, SocketAddr};
use async_std::prelude::*;
use async_std::task;
use std::io::Error;

use futures::future::{Either, select};

use crate::auth::authenticate;
use crate::bridge::run_bridge;
use crate::keys::Key;

pub async fn run_server(port: u16, adapter_port: u16, key: Key) -> Result<(), Error> {

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = TcpListener::bind(socket_addr).await?;

    // There is always an ongoing task that accepts an incoming connection on the clear (not adapter) port
    // This task is replaced when the socket is accepted
    let mut incoming_future = task::spawn(accept(listener));

    let adapter_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), adapter_port);
    let adapter_listener = TcpListener::bind(adapter_socket_addr).await?;

    log::info!("Bounce server: Listening for incoming connections on {}, accepting adapter on port {}", port, adapter_port);
    
    'adapter_accept: loop {
        let (mut adapter_stream, _) = adapter_listener.accept().await?;

        log::info!("Incoming adapter stream: {:?}", adapter_stream.peer_addr().unwrap());

        let xors = match authenticate(key.clone(), adapter_stream.clone()).await {
            Err(err) => {
                log::error!("Bad client: {}", err);
                match adapter_stream.shutdown(Shutdown::Both) {
                    Err(err) => log::error!("Problem shutting down socket after an authentication error: {}", err),
                    Ok(_) => {}
                }
                continue 'adapter_accept;
            },
            Ok(n) => n
        };

        let stream;
        
        // This complicated loop:
        // - Accepts the incoming stream (via incoming_future)
        // - ALSO waits to see if the adapter_stream terminates (via peek_future)
        // What happens is:
        // - if incoming_future (incoming clear stream) completes first, incoming_future is re-assigned a new task to accept the next stream
        // - if peek_future (waits to see if adapter_stream terminates) completes first, then adapter_stream is cleaned up and the 'adapter_accept runs again

        // Worth noting: If we don't need to handle adapter_stream ending, this is significantly simpler!


        let mut peek_future = task::spawn(peek(adapter_stream.clone()));
        'accept: loop {

            match select(incoming_future, peek_future).await {
                Either::Left(((listener, result), p)) => {
                    incoming_future = task::spawn(accept(listener));
                    match result {
                        Ok(s) => {
                            stream = s;
                            break 'accept;
                        },
                        Err(err) => {
                            log::error!("Error accepting incoming stream: {}", err);
                            peek_future = p;
                        }
                    }
                },
                Either::Right((peek_result, i)) => {
                    incoming_future = i;
                    match peek_result {
                        Ok(bytes_sent) => {
                            let shutdown_result = if bytes_sent > 0 {
                                log::warn!("Adapter stream sent unexpected data: {:?}", adapter_stream.peer_addr().unwrap());
                                adapter_stream.shutdown(Shutdown::Both)
                            } else {
                                log::info!("Adapter stream ended: {:?}", adapter_stream.peer_addr().unwrap());
                                adapter_stream.shutdown(Shutdown::Write)
                            };

                            match shutdown_result {
                                Ok(_) => {},
                                Err(err) => log::error!("Error shutting down adapter stream: {:?}:, {}", adapter_stream.peer_addr().unwrap(), err)
                            }

                            continue 'adapter_accept;
                        },
                        Err(err) => {
                            log::error!("Adapter stream aborted: {}", err);
                            continue 'adapter_accept;
                        }
                    }
                },
            };
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

async fn accept(listener: TcpListener) -> (TcpListener, Result<TcpStream, Error>) {
    match listener.accept().await {
        Ok((s, _)) => (listener, Ok(s)),
        Err(err) => (listener, Err(err))
    }
}

async fn peek(stream: TcpStream) -> Result<usize, Error> {
    let mut peek_buf = [0u8; 1];
    let bytes = stream.peek(&mut peek_buf).await?;
    Ok(bytes)
}

