use async_std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, SocketAddr};
use async_std::prelude::*;
use std::io::{Error, ErrorKind};

use crypto::aes;
//use crypto::aes::KeySize;
use rand::{RngCore, thread_rng};

use crate::bridge::bridge;
use crate::keys::Key;

pub async fn run_server(port: u16, adapter_port: u16, _key: Key) {

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

                // TODO: Authentication

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
/*
async fn authenticate(key: &Key, mut adapter_stream: &TcpStream) -> Result<Vec<u8>, Error> {

    match adapter_stream.write_all(b"bounce").await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    let mut nonce = vec![0u8; key.key.len()];
    thread_rng().fill_bytes(&mut nonce);

    match adapter_stream.write_all(&nonce).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    let mut challenge_encrypted = vec![0u8; key.key.len()];
    let mut bytes_read = 0;
    'readloop: loop {
        match adapter_stream.read(&mut challenge_encrypted).await {
            Err(err) => return Err(err),
            Ok(b) => {
                if b == 0 {
                    return Err(Error::new(ErrorKind::InvalidData, "Challenge not sent"));
                }

                bytes_read = bytes_read + b;
                if bytes_read == 16 {
                    break 'readloop;
                }
            }
        }
    }

    let mut cipher = aes::ctr(key.size, &key.key, &nonce);

    let mut output = vec![0u8; key.key.len()]; //Vec<u8> = Vec:: //repeat(0u8).take(secret.len()).collect();
    cipher.process(&challenge_encrypted, &mut output[..]);

    for ctr in 0..output.len() {
        output[ctr] = output[ctr] ^ 0xff;
    }

    match adapter_stream.write_all(&output).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    // https://crates.io/crates/cryptostream
    // https://zsiciarz.github.io/24daysofrust/book/vol1/day21.html

    Ok(nonce)
}
*/