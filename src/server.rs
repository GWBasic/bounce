use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
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

    //let mut adapter_incoming = adapter_listener.incoming();

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
        
        'accept: loop {
            match listener.accept().await {
                Err(err) => log::error!("Error accepting incoming stream: {}", err),
                Ok((s, _)) => {
                    stream = s;
                    break 'accept;
                }
            };

            /*let mut peek_buf = [0u8; 1];

            let incoming_future = listener.accept();
            let incoming_future = task::spawn(incoming_future);
            let peek_future = adapter_stream.clone().peek(&mut peek_buf);
            let peek_future = task::spawn(peek_future);

            match select(incoming_future, peek_future).await {
                Either::Left(r) => match r.0 {
                    Ok((s, _)) => {
                        stream = s;
                        break 'accept;
                    },
                    Err(err) => log::error!("Error accepting incoming stream: {}", err)
                },
                Either::Right(r) => match r.0 {
                    Ok(_) => {
                        log::error!("Adapter stream sent unexpected data");
                        adapter_stream.shutdown(Shutdown::Both)?;
                        continue 'adapter_accept;
                    },
                    Err(err) => {
                        log::error!("Adapter stream complete: {}", err);
                        continue 'adapter_accept;
                    }
                },
            };*/
        
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

