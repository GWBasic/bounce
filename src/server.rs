use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream, SocketAddr};
use async_std::prelude::*;
use async_std::task;
use async_std::task::JoinHandle;
use std::io::{ Error, ErrorKind };
use sync_tokens::cancelation_token::{ CancelationToken, Cancelable };
use sync_tokens::completion_token::{ CompletionToken, Completable };

use futures::future::{Either, select};

use crate::auth::authenticate;
use crate::bridge::run_bridge;
use crate::keys::Key;

pub fn run_server(port: u16, adapter_port: u16, key: Key) -> (JoinHandle<Result<(), Error>>, CompletionToken<()>, CancelationToken) {
    let (listening_token, listening_completable) = CompletionToken::new();
    let (cancelation_token, cancelable) = CancelationToken::new();

    let server_future = task::spawn(run_server_int(port, adapter_port, key, listening_completable, cancelable));

    (server_future, listening_token, cancelation_token)
}

async fn run_server_int(port: u16, adapter_port: u16, key: Key, listening_completable: Completable<()>, cancelable: Cancelable) -> Result<(), Error> {

    let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = TcpListener::bind(socket_addr).await?;

    // There is always an ongoing task that accepts an incoming connection on the clear (not adapter) port
    // This task is replaced when the socket is accepted
    let mut incoming_future = task::spawn(accept(listener));

    let adapter_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), adapter_port);
    let adapter_listener = TcpListener::bind(adapter_socket_addr).await?;

    // This is the ongoing task to wait for adapter sockets
    let mut incoming_adapter_future = task::spawn(accept(adapter_listener));

    listening_completable.complete(());

    log::info!("Bounce server: Listening for incoming connections on {}, accepting adapter on port {}", port, adapter_port);
    
    'adapter_accept: loop {

        let (listener, mut adapter_stream) = cancelable.allow_cancel(incoming_adapter_future, Err(Error::new(ErrorKind::Interrupted, "Server terminated"))).await?;
        incoming_adapter_future = task::spawn(accept(listener));

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


        let peek_future = task::spawn(peek(adapter_stream.clone()));

        match select(incoming_future, select(peek_future, cancelable.future())).await {
            Either::Left((r, _)) => {
                let (listener, s) = r?;
                incoming_future = task::spawn(accept(listener));
                stream = s;
            },
            Either::Right((select_result, i)) => {
                match select_result {
                    Either::Left((peek_result, _)) => {
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
                    Either::Right(_) => return Err(Error::new(ErrorKind::Interrupted, "Server terminated"))
                }
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

async fn accept(listener: TcpListener) -> Result<(TcpListener, TcpStream), Error> {
    match listener.accept().await {
        Ok((s, _)) => Ok((listener, s)),
        Err(err) => Err(err)
    }
}

async fn peek(stream: TcpStream) -> Result<usize, Error> {
    let mut peek_buf = [0u8; 1];
    let bytes = stream.peek(&mut peek_buf).await?;
    Ok(bytes)
}

// Note: Tests are error conditions only, happy-path tests are in main.rs
#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
    use async_std::prelude::*;
    use async_std::task::JoinHandle;
    use std::io::Error;

    use crypto::aes::KeySize;

    use super::*;

    async fn get_adapter_stream_and_server_future() -> (TcpStream, SocketAddr, JoinHandle<Result<(), Error>>, CancelationToken) {
        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            size: KeySize::KeySize256
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();
        let adapter_listener = TcpListener::bind(socket_addr).await.unwrap();

        let client_address = listener.local_addr().unwrap();
        let adapter_address = adapter_listener.local_addr().unwrap();

        drop(listener);
        drop(adapter_listener);

        let (server_future, listening_token, cancelation_token) = run_server(client_address.port(), adapter_address.port(), key.clone());

        listening_token.await;

        let adapter_stream = TcpStream::connect(adapter_address).await.expect("Can not connect to the server");

        authenticate(key, adapter_stream.clone()).await.expect("Can not authenticate client stream");

        (adapter_stream, adapter_address, server_future, cancelation_token)
    }

    #[async_std::test]
    async fn client_drops_connection() {

        let (adapter_stream, adapter_address, server_future, cancelation_token) = get_adapter_stream_and_server_future().await;

        adapter_stream.shutdown(Shutdown::Both).expect("Can not shut down client stream");

        let adapter_stream = TcpStream::connect(adapter_address).await.expect("Can not connect to the server");
        adapter_stream.shutdown(Shutdown::Both).expect("Can not shut down client stream");

        cancelation_token.cancel();
        let err = server_future.await.expect_err("Server should terminate");
        assert_eq!(ErrorKind::Interrupted, err.kind(), "");
    }

    #[async_std::test]
    async fn client_unexpected_write_connection() {

        let (mut adapter_stream, adapter_address, server_future, cancelation_token) = get_adapter_stream_and_server_future().await;

        let buf = [0u8];
        adapter_stream.write_all(&buf).await.expect("Can not write to client stream");

        let adapter_stream = TcpStream::connect(adapter_address).await.expect("Can not connect to the server");
        adapter_stream.shutdown(Shutdown::Both).expect("Can not shut down client stream");

        cancelation_token.cancel();
        let err = server_future.await.expect_err("Server should terminate");
        assert_eq!(ErrorKind::Interrupted, err.kind(), "");
    }
}

