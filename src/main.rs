mod auth;
mod bridge;
mod client;
mod completion_token;
mod keys;
mod server;
mod xor;

use std::env::{args, var};
use std::io::{ Error, ErrorKind, Write };

use chrono::Local;
use env_logger::Builder;
use log::LevelFilter;

use client::run_client;
use completion_token::CompletionToken;
use keys::{Key, generate_keys, parse_key};
use server::run_server;

#[async_std::main]
async fn main() {
    println!("Bounce");

    setup_logging();

    let result = match var("BOUNCE_MODE") {
        Ok(mode) => main_env(mode).await,
        Err(_) => main_args().await
    };

    match result {
        Ok(()) => {},
        Err(err) => log::error!("Bounce terminated in error:\n\t{}", err)
    }
}

fn setup_logging() {
    Builder::new()
        .parse_env("BOUNCE_LOG")
        .format(|buf, record| {
            writeln!(buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();
}

async fn main_env(mode: String) -> Result<(), Error> {

    // Environment errors are logged, because it's assumed these need to go into a standard logger

    match parse_mode(&mode) {
        Mode::Server => {
            let port = get_port_from_env("BOUNCE_PORT")?;
            let adapter_port = get_port_from_env("BOUNCE_ADAPTER_PORT")?;
            let key = get_key_from_env("BOUNCE_KEY")?;
        
            run_server(port, adapter_port, key, CompletionToken::new(), CompletionToken::new()).await?;
        },
        Mode::Client => {
            let bounce_server = get_env_var("BOUNCE_SERVER")?;
            let destination_host = get_env_var("BOUNCE_DESTINATION_HOST")?;
            let key = get_key_from_env("BOUNCE_KEY")?;

            run_client(bounce_server, destination_host, key).await?;
        },
        Mode::Keys => {
            generate_keys();
        }
    }

    Ok(())
}

async fn main_args() -> Result<(), Error> {
    let args: Vec<String> = args().collect();

    // Panics are used instead of logging because it's assumed that bounce is being run interactively

    if args.len() < 2 {
        panic!("Must pass the mode (Server or Client) as the first argument");
    }

    match parse_mode(&args[1]) {
        Mode::Server => {
            if args.len() != 5 {
                panic!("Please specify the ports as command-line arguments:\n\t bounce server [port] [adapter port] [key]");
            }
        
            let port = parse_port(&args[2]).unwrap();
            let adapter_port = parse_port(&args[3]).unwrap();
            let key = parse_key(&args[4]);
        
            run_server(port, adapter_port, key, CompletionToken::new(), CompletionToken::new()).await?;
        },
        Mode::Client => {

            if args.len() != 5 {
                panic!("Please specify the host and port as command-line arguments:\n\t bounce client [bounce server:port] [destination:port] [key]");
            }
        
            let bounce_server = args[2].clone();
            let destination_host = args[3].clone();
            let key = parse_key(&args[4]);
        
            run_client(bounce_server, destination_host, key).await?;
        },
        Mode::Keys => {
            if args.len() != 2 {
                panic!("Generating keys takes no arguments");
            }

            generate_keys();
        }
    }

    Ok(())
}

fn get_env_var(var_name: &str) -> Result<String, Error> {
    match var(var_name) {
        Ok(val) => Ok(val),
        Err(_) => Err(Error::new(ErrorKind::Other, format!("{} must be set", var_name)))
    }
}

fn get_port_from_env(var_name: &str) -> Result<u16, Error> {
    let port_str = get_env_var(var_name)?;
    Ok(parse_port(&port_str)?)
}

fn parse_port(port_str: &str) -> Result<u16, Error> {
    match port_str.parse::<u16>() {
        Ok(port) => Ok(port),
        Err(err) => Err(Error::new(ErrorKind::Other, format!("Invalid port \"{}\": {}", port_str, err)))
    }
}

fn get_key_from_env(var_name: &str) -> Result<Key, Error> {
    let key_str = get_env_var(var_name)?;
    Ok(parse_key(&key_str))
}

enum Mode {
    Server,
    Client,
    Keys
}

fn parse_mode(mode: &String) -> Mode {
    if mode == "server" {
        Mode::Server
    } else if mode == "client" {
        Mode::Client
    } else if mode == "keys" {
        Mode::Keys
    } else {
        panic!("Unknown mode: {}", mode);
    }
}

#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream, SocketAddr};
    use async_std::prelude::*;
    use async_std::task;
    use async_std::task::JoinHandle;
    use std::io::{Error, ErrorKind};

    use crypto::aes::KeySize;
    use rand::{RngCore, thread_rng};

    use super::*;

    async fn get_server_and_client_futures() -> (JoinHandle<Result<(), Error>>, JoinHandle<Result<(), Error>>, SocketAddr, CompletionToken, CompletionToken, TcpListener) {
        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            size: KeySize::KeySize256
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let client_listener = TcpListener::bind(socket_addr).await.unwrap();
        let adapter_listener = TcpListener::bind(socket_addr).await.unwrap();

        let client_address = client_listener.local_addr().unwrap();
        let adapter_address = adapter_listener.local_addr().unwrap();

        drop(client_listener);
        drop(adapter_listener);

        let listening_token = CompletionToken::new();
        let cancelation_token = CompletionToken::new();

        let server_future = task::spawn(run_server(client_address.port(), adapter_address.port(), key.clone(), listening_token.clone(), cancelation_token.clone()));

        let listener = TcpListener::bind(socket_addr).await.unwrap();
        let client_future = task::spawn(run_client(adapter_address.to_string(), listener.local_addr().unwrap().to_string(), key.clone()));

        (server_future, client_future, client_address, listening_token, cancelation_token, listener)


        /*let key = Key {
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

        (server_stream, client_future)*/
    }

    /*
    #[async_std::test]
    async fn server_drops_connection() {

        let (server_stream, client_future) = get_server_stream_and_client_future().await;

        server_stream.shutdown(Shutdown::Both).expect("Can not shut down server stream");

        let err = client_future.await.expect_err("The client should end in error");

        assert_eq!(err.kind(), ErrorKind::ConnectionRefused);
    }*/

    #[async_std::test]
    async fn happy_path() {
        /*let (server_future, client_future, client_address, listening_token, cancelation_token, listener) = get_server_and_client_futures().await;

        let outgoing_stream = TcpStream::connect(client_address).await.expect("Can't connect");
        let (incoming_stream, _) = listener.accept().await.expect("Incoming socket didn't come");

        let mut rng = thread_rng();

        let mut a = outgoing_stream.clone();
        let mut b = incoming_stream.clone();

        for _ in 0usize..100 {
            let len = (rng.next_u64() % 2000) as usize;
            let mut write_buf = vec!(0u8; len);
            rng.fill_bytes(&mut write_buf);

            let write_future = task::spawn(a.write_all(&write_buf));

            let mut read_buf = vec!(0u8; len);

            let mut total_bytes_read = 0;

            'read_loop: loop {
                let bytes_read = b.read(&mut read_buf).await.expect("Can't read");

                if bytes_read == 0 {
                    panic!("Socket closed early")
                }

                total_bytes_read = total_bytes_read + bytes_read;

                if total_bytes_read >= len {
                    break 'read_loop;
                }
            }

            write_future.await.expect("Problem writing");

            assert_eq!(write_buf, read_buf, "Contents garbled");

            let c = a;
            a = b;
            b = c;
        }*/

        panic!("Incomplete");
    }
}