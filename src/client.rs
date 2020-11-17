use async_std::net::TcpStream;
use async_std::prelude::*;

use crate::bridge::bridge;

pub async fn run_client(bounce_server: String, destination_host: String) {
    println!("Bounce client: Connecting to bounce server at {}, bouncing to {}", bounce_server, destination_host);

    'client_loop: loop {
        match TcpStream::connect(bounce_server.clone()).await {
            Err(err) => {
                println!("Can not connect to bounce server \"{}\": {}", bounce_server, err);
                break 'client_loop;
            },
            Ok(mut bounce_stream) => {

                // TODO: Authentication

                let mut buf: [u8; 9] = [0; 9];
                let mut read = 0;

                while read < 9 {
                    match bounce_stream.read(&mut buf[read..9]).await {
                        Err(err) => {
                            println!("Problem with connection to bounce server \"{}\": {}", bounce_server, err);
                            continue 'client_loop;
                        },
                        Ok(r) => read = read + r
                    }
                }

                if b"connected" != &buf {
                    println!("Bounce server did not initiate the connection correctly");
                    continue 'client_loop;
                }

                match TcpStream::connect(destination_host.clone()).await {
                    Err(err) => {
                        println!("Can not connect to host \"{}\": {}", destination_host, err);
                        break 'client_loop;
                    },
                    Ok(destination_stream) => {

                        println!("Bridging connection");

                        bridge(bounce_stream, "bounce-incoming".to_string(), destination_stream, "outgoing".to_string());
                    }
                }        
            }
        }
    }
}