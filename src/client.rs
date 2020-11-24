use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use std::io::Error;

use crate::auth::authenticate;
use crate::bridge::run_bridge;
use crate::keys::Key;

pub async fn run_client(bounce_server: String, destination_host: String, key: Key) -> Result<(), Error> {
    log::info!("Bounce client: Connecting to bounce server at {}, bouncing to {}", bounce_server, destination_host);

    let connected = b"connected".to_vec();

    'client_loop: loop {
        let mut bounce_stream = TcpStream::connect(bounce_server.clone()).await?;

        let xors = authenticate(key.clone(), bounce_stream.clone()).await?;

        let mut buf = vec!(0u8; connected.len());
        let mut read = 0;

        'read_loop: loop {
            // TODO: This read should have a timeout
            let r = bounce_stream.read(&mut buf[read..]).await?;

            if r == 0 {
                log::error!("Connection to bounce server {} ended", bounce_server);
                bounce_stream.shutdown(Shutdown::Write)?;
                continue 'client_loop;
            }

            read = read + r;

            if read >= connected.len() {
                break 'read_loop;
            }
        }

        if connected != buf {
            log::error!("Bounce server did not initiate the connection correctly");
            bounce_stream.shutdown(Shutdown::Both)?;
            continue 'client_loop;
        }

        match TcpStream::connect(destination_host.clone()).await {
            Err(err) => {
                log::error!("Can not connect to host \"{}\": {}", destination_host, err);
                break 'client_loop;
            },
            Ok(destination_stream) => {

                log::info!("Bridging connection");

                run_bridge(xors, destination_stream, "outgoing".to_string(), bounce_stream, "bounce-incoming".to_string());
            }
        }        
    }

    Ok(())
}

// Note: Tests are error conditions only, happy-path tests will be handled in general integration tests
#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
    use async_std::prelude::*;
    use async_std::task;
    use async_std::task::JoinHandle;
    use std::io::{Error, ErrorKind};

    use crypto::aes::KeySize;

    use super::*;

    async fn get_server_stream_and_client_future() -> (TcpStream, JoinHandle<Result<(), Error>>) {
        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            size: KeySize::KeySize256
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_future = task::spawn(run_client(local_addr.to_string(), "no destination".to_string(), key.clone()));

        let server_stream = listener.incoming().next().await.unwrap().expect("Did not get incoming connection from the client");
        drop(listener);

        authenticate(key, server_stream.clone()).await.expect("Can not authenticate server stream");

        (server_stream, client_future)
    }

    #[async_std::test]
    async fn server_drops_connection() {

        let (server_stream, client_future) = get_server_stream_and_client_future().await;

        server_stream.shutdown(Shutdown::Both).expect("Can not shut down server stream");

        let err = client_future.await.expect_err("The client should end in error");

        assert_eq!(err.kind(), ErrorKind::ConnectionRefused);
    }

    #[async_std::test]
    async fn server_sends_incorrect_token() {

        let (mut server_stream, client_future) = get_server_stream_and_client_future().await;

        server_stream.write_all(b"xxxxxxxxx").await.expect("Can not send incorrect data");

        let err = client_future.await.expect_err("The client should end in error");

        assert_eq!(err.kind(), ErrorKind::ConnectionRefused);

        server_stream.shutdown(Shutdown::Write).expect("Can not shut down server stream");
    }

    // TODO: Test timeout
}

