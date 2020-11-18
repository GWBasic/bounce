use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use async_std::task;
use core::time::Duration;
use std::io::{Error, ErrorKind};

use crypto::aes;
use futures::future::FutureExt;
use futures::pin_mut;
use futures::select;
use rand::{RngCore, thread_rng};

use crate::keys::Key;

pub fn bridge(a: TcpStream, a_name: String, b: TcpStream, b_name: String) {

    match a.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", a_name, err);
            return;
        },
        Ok(()) => {}
    }

    match b.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", b_name, err);
            return;
        },
        Ok(()) => {}
    }

    task::spawn(bridge_connections(a.clone(), a_name.clone(), b.clone(), b_name.clone()));
    task::spawn(bridge_connections(b, b_name, a, a_name));

    // TODO: Await and log
}

async fn bridge_connections(mut reader: TcpStream, reader_name: String, mut writer: TcpStream, writer_name: String)  {
    
    let mut buf: [u8; 4096] = [0; 4096];

    'bridge: loop {
        match reader.read(&mut buf).await {
            Err(err) => {
                println!("Reading {} stopped: {}", reader_name, err);                
                break 'bridge;
            },
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    println!("{} complete", reader_name);
                    break 'bridge;
                }

                println!("Read {} bytes from {}", bytes_read, reader_name);

                let write_slice = &buf[0..bytes_read];
                match writer.write_all(write_slice).await {
                    Err(err) => {
                        println!("Writing {} stopped: {}", writer_name, err);
                        break 'bridge;
                    },
                    Ok(()) => {
                        println!("Wrote {} bytes to {}", bytes_read, writer_name);
                    }
                }
            }
        }
    }

    match writer.flush().await {
        Err(err) => {
            println!("Can not flush: {}", err);
        },
        Ok(()) =>{}
    }

    match reader.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", reader_name),
        Err(err) => println!("Error shutting down {}: {}", reader_name, err)
    }

    match writer.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", writer_name),
        Err(err) => println!("Error shutting down {}: {}", writer_name, err)
    }
}


async fn read_buffer(mut stream: &TcpStream, buffer: &mut [u8], timeout: Duration) -> Result<(), Error> {
    let mut bytes_read = 0;
    loop {
        let read_future = stream.read(buffer).fuse();
        let timeout_future = task::sleep(timeout).fuse();

        pin_mut!(read_future, timeout_future);

        select! {
            read_result = read_future => match read_result {
                Err(err) => return Err(err),
                Ok(b) => {
                    if b == 0 {
                        return Err(Error::new(ErrorKind::InvalidData, "Socket closed prematurely"));
                    }
    
                    bytes_read = bytes_read + b;
                    if bytes_read == buffer.len() {
                        return Ok(());
                    }
                }
            },
            _ = timeout_future => return Err(Error::new(ErrorKind::TimedOut, "timeout"))
        }       
    }
}

async fn write_buffer(mut stream: TcpStream, buffer: &[u8]) -> Result<(), Error> {
    stream.write_all(buffer).await
}

async fn authenticate(_key: Key, adapter_stream: TcpStream) -> Result<(), Error> {

    // Read and write "bounce"
    let write_future = task::spawn(write_buffer(adapter_stream.clone(), b"bounce"));

    let mut bounce_buffer = [0u8; b"bounce".len()];
    // TODO: Timeout
    match read_buffer(&adapter_stream, &mut bounce_buffer, Duration::from_secs_f32(0.5)).await {
        Err(err) => return Err(err),
        Ok(_) => {}
    };

    if bounce_buffer[..] != b"bounce"[..] {
        return Err(Error::new(ErrorKind::InvalidData, "This is not a bounce server or client"));
    }

    match write_future.await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    /*
    let mut nonce = vec![0u8; key.key.len()];
    thread_rng().fill_bytes(&mut nonce);

    match adapter_stream.write_all(&nonce).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    let mut challenge_encrypted = vec![0u8; key.key.len()];
    match read_buffer(&adapter_stream, &mut challenge_encrypted).await {
        Err(err) => return Err(err),
        Ok(_) => {}
    };

    let mut cipher = aes::ctr(key.size, &key.key, &nonce);

    let mut output = vec![0u8; key.key.len()];
    cipher.process(&challenge_encrypted, &mut output[..]);
    match adapter_stream.write_all(&output).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    for ctr in 0..output.len() {
        output[ctr] = output[ctr] ^ 0xff;
    }

    match adapter_stream.write_all(&output).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    let mut challenge = vec![0u8; key.key.len()];
    thread_rng().fill_bytes(&mut challenge);
    cipher.process(&challenge_encrypted, &mut output[..]);

    match adapter_stream.write_all(&output).await {
        Err(err) => return Err(err),
        Ok(()) => {}
    }

    match read_buffer(&adapter_stream, &mut challenge_encrypted).await {
        Err(err) => return Err(err),
        Ok(_) => {}
    };

    for ctr in 0..challenge_encrypted.len() {
        let expected_value = challenge[ctr] ^ 0xff;
        if challenge_encrypted[ctr] != expected_value {
            return Err(Error::new(ErrorKind::InvalidData, "Challenge failed"));
        }
    }

    // https://crates.io/crates/cryptostream
    // https://zsiciarz.github.io/24daysofrust/book/vol1/day21.html
    */

    Ok(())
}


#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, TcpListener, SocketAddr};
    use async_std::prelude::*;

    use crypto::aes::KeySize;

    use super::*;

    #[async_std::test]
    async fn authenticate_works() {

        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(local_addr).await.unwrap();
        let server_stream = listener.incoming().next().await.unwrap().unwrap();

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_authenticate_future = task::spawn(authenticate(key.clone(), server_stream));

        client_authenticate_future.await.unwrap();
        server_authenticate_future.await.unwrap();
    }

    #[async_std::test]
    async fn authenticate_wrong_id() {

        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(local_addr).await.unwrap();
        let server_stream = listener.incoming().next().await.unwrap().unwrap();

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_emulate_future = task::spawn(write_buffer(server_stream.clone(), b"boXXce"));

        server_emulate_future.await.unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!("This is not a bounce server or client", err.to_string())
        }
    }

    #[async_std::test]
    async fn authenticate_short_id() {

        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(local_addr).await.unwrap();
        let server_stream = listener.incoming().next().await.unwrap().unwrap();

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_emulate_future = task::spawn(write_buffer(server_stream.clone(), b"short"));

        server_emulate_future.await.unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::TimedOut, err.kind())
        }
    }

    #[async_std::test]
    async fn authenticate_shutdown_immediate() {

        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(local_addr).await.unwrap();
        let server_stream = listener.incoming().next().await.unwrap().unwrap();

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        server_stream.shutdown(Shutdown::Both).unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::InvalidData, err.kind())
        }
    }
}