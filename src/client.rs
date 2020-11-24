use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use std::io::Error;

use crate::auth::authenticate;
use crate::bridge::run_bridge;
use crate::keys::Key;

pub async fn run_client(bounce_server: String, destination_host: String, key: Key) -> Result<(), Error> {
    log::info!("Bounce client: Connecting to bounce server at {}, bouncing to {}", bounce_server, destination_host);

    'client_loop: loop {
        let mut bounce_stream = TcpStream::connect(bounce_server.clone()).await?;

        let xors = authenticate(key.clone(), bounce_stream.clone()).await?;

        let mut buf: [u8; 9] = [0; 9];
        let mut read = 0;

        while read < 9 {
            match bounce_stream.read(&mut buf[read..9]).await {
                Err(err) => {
                    log::error!("Problem with connection to bounce server \"{}\": {}", bounce_server, err);
                    continue 'client_loop;
                },
                Ok(r) => read = read + r
            }
        }

        if b"connected" != &buf {
            log::error!("Bounce server did not initiate the connection correctly");

            match bounce_stream.shutdown(Shutdown::Both) {
                Ok(()) => {},
                Err(err) => log::error!("Error shutting down bounce_stream: {}", err)
            }

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