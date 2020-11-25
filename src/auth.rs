use async_std::io;
use async_std::io::{Read, Write};
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task;
use core::time::Duration;
use std::any::Any;
use std::convert::TryInto;
use std::io::{Error, ErrorKind};
use std::marker::Unpin;

use crypto::aes;
use futures::future::FutureExt;
use futures::pin_mut;
use futures::select;
use rand::{RngCore, thread_rng};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;

use crate::keys::Key;
use crate::xor::{Xor, Xors};

pub async fn authenticate(key: Key, stream: TcpStream) -> Result<Xors<ChaCha12Rng>, Error> {

    // TODO: A potential optimization is to send "bounce", nonce, and challenges as one single write

    // Read and write "bounce"
    let bounce_buffer = read_and_write(stream.clone(), &b"bounce".to_vec(), Duration::from_secs_f32(0.5)).await?;
    if bounce_buffer[..] != b"bounce"[..] {
        return Err(Error::new(ErrorKind::InvalidData, "This is not a bounce server or client"));
    }

    // Read and write nonces
    let mut my_nonce = vec![0u8; key.key.len()];
    thread_rng().fill_bytes(&mut my_nonce);

    let their_nonce = read_and_write(stream.clone(), &my_nonce, Duration::from_secs_f32(0.5)).await?;

    // Read and write seeds
    let mut my_seed: <ChaCha12Rng as SeedableRng>::Seed = Default::default();
    thread_rng().fill(&mut my_seed);
    let my_seed_encrypted = process(&key, &my_nonce, &my_seed);

    let their_seed_encrypted = read_and_write(stream.clone(), &my_seed_encrypted, Duration::from_secs_f32(0.5)).await?;

    let their_seed = process(&key, &their_nonce, &their_seed_encrypted)[0..32].try_into().expect("Unexpected seed size");
    let their_seed = <ChaCha12Rng as SeedableRng>::Seed::from(their_seed);

    let write_rng = ChaCha12Rng::from_seed(my_seed);
    let read_rng = ChaCha12Rng::from_seed(their_seed);

    // Create the xors
    let mut write_xor = Xor::new(write_rng);
    let mut read_xor = Xor::new(read_rng);

    // Read and write encrypted "bounce"
    let mut my_bounce = b"bounce".to_vec();
    write_xor.process(&mut my_bounce[..]);

    let mut their_bounce = read_and_write(stream.clone(), &my_bounce, Duration::from_secs_f32(0.5)).await.expect("Handshake error");
    read_xor.process(&mut their_bounce[..]);

    if their_bounce[..] != b"bounce"[..] {
        return Err(Error::new(ErrorKind::InvalidData, "Authentication failed"));
    }

    Ok(Xors {
        write_xor,
        read_xor
    })
}

async fn read_buffer<TStream>(mut stream: TStream, buffer: &mut [u8], timeout: Duration) -> Result<(), Error>
where TStream : Read + Write + Unpin {
    let mut total_bytes_read = 0;
    loop {
        let bytes_read = io::timeout(timeout, stream.read(buffer)).await?;

        if bytes_read == 0 {
            return Err(Error::new(ErrorKind::InvalidData, "Socket closed prematurely"));
        }

        total_bytes_read = total_bytes_read + bytes_read;
        if total_bytes_read >= buffer.len() {
            return Ok(());
        }
    }
}

async fn write_buffer<TStream>(mut stream: TStream, buffer: Vec<u8>) -> Result<(), Error>
where TStream : Read + Write + Unpin {
    stream.write_all(&buffer).await
}

async fn read_and_write<TStream>(stream: TStream, buffer_to_write: &Vec<u8>, timeout: Duration) -> Result<Vec<u8>, Error>
where TStream : Read + Write + Unpin + Clone + Send + Any {

    let write_future = task::spawn(write_buffer(stream.clone(), buffer_to_write.clone()));

    let mut buffer_to_read = vec![0u8; buffer_to_write.len()];

    read_buffer(stream, &mut buffer_to_read, timeout).await?;

    write_future.await?;
    Ok(buffer_to_read)
}

fn process(key: &Key, nonce: &Vec<u8>, to_process: &[u8]) -> Vec<u8> {
    let mut my_ciper = aes::ctr(key.size, &key.key, nonce);
    let mut processed = vec![0u8; to_process.len()];
    my_ciper.process(to_process, &mut processed);

    processed
}

#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
    use async_std::prelude::*;

    use crypto::aes::KeySize;

    use super::*;

    async fn get_key_and_socket_streams() -> (Key, TcpStream, TcpStream) {
        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            size: KeySize::KeySize256
        };

        let (client_stream, server_stream) = get_socket_streams().await;

        (key, client_stream, server_stream)
    }

    async fn get_socket_streams() -> (TcpStream, TcpStream) {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(local_addr).await.unwrap();
        let server_stream = listener.incoming().next().await.unwrap().unwrap();

        (client_stream, server_stream)
    }

    #[async_std::test]
    async fn authenticate_works() {

        let (key, client_stream, server_stream) = get_key_and_socket_streams().await;

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_authenticate_future = task::spawn(authenticate(key.clone(), server_stream));

        client_authenticate_future.await.unwrap();
        server_authenticate_future.await.unwrap();
    }

    #[async_std::test]
    async fn authenticate_wrong_id() {

        let (key, client_stream, server_stream) = get_key_and_socket_streams().await;

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_emulate_future = task::spawn(write_buffer(server_stream.clone(), b"boXXce".to_vec()));

        server_emulate_future.await.unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!("This is not a bounce server or client", err.to_string())
        }
    }

    #[async_std::test]
    async fn authenticate_short_id() {

        let (key, client_stream, server_stream) = get_key_and_socket_streams().await;

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        let server_emulate_future = task::spawn(write_buffer(server_stream.clone(), b"short".to_vec()));

        server_emulate_future.await.unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::TimedOut, err.kind())
        }
    }

    #[async_std::test]
    async fn authenticate_shutdown_immediate() {

        let (key, client_stream, server_stream) = get_key_and_socket_streams().await;

        let client_authenticate_future = task::spawn(authenticate(key.clone(), client_stream));
        server_stream.shutdown(Shutdown::Both).unwrap();
        
        match client_authenticate_future.await {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::InvalidData, err.kind())
        }
    }

    #[async_std::test]
    async fn authenticate_different_keys() {

        let key_1 = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            size: KeySize::KeySize128
        };

        let key_2 = Key {
            key: vec![2 as u8, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33],
            size: KeySize::KeySize128
        };

        let (client_stream, server_stream) = get_socket_streams().await;

        let client_authenticate_future = task::spawn(authenticate(key_1, client_stream));
        let server_authenticate_future = task::spawn(authenticate(key_2, server_stream));

        let client_authenticate_result = client_authenticate_future.await;
        let server_authenticate_result = server_authenticate_future.await;

        match client_authenticate_result {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::InvalidData, err.kind())
        }

        match server_authenticate_result {
            Ok(_) => panic!("Failure not detected"),
            Err(err) => assert_eq!(ErrorKind::InvalidData, err.kind())
        }
    }

    async fn read_and_write_take(stream: TcpStream, buffer_to_write: Vec<u8>, timeout: Duration) -> Result<Vec<u8>, Error> {
        read_and_write(stream, &buffer_to_write, timeout).await
    }

    #[async_std::test]
    async fn verify_read_and_write() {
        let a = vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let b = vec![10 as u8, 9, 8, 7, 6, 5, 4, 3, 2, 1];

        let (client_stream, server_stream) = get_socket_streams().await;

        let a_sent_future = task::spawn(read_and_write_take(server_stream.clone(), b.clone(), Duration::from_secs_f32(0.5)));
        let b_sent = read_and_write_take(client_stream, a.clone(), Duration::from_secs_f32(0.5)).await.unwrap();
        let a_sent = a_sent_future.await.unwrap();

        assert_eq!(a, a_sent);
        assert_eq!(b, b_sent);
    }

    #[test]
    fn different_keys() {

        let key_1 = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let key_2 = Key {
            key: vec![2 as u8, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
            size: KeySize::KeySize128
        };

        let mut secret = vec![0u8; key_1.key.len()];
        thread_rng().fill_bytes(&mut secret);

        let mut nonce = vec![0u8; key_1.key.len()];
        thread_rng().fill_bytes(&mut nonce);

        let encrypted = process(&key_1, &nonce, &secret);
        
        let decrypted = process(&key_2, &nonce, &encrypted);
        assert_ne!(secret, decrypted);

        let decrypted = process(&key_1, &nonce, &encrypted);
        assert_eq!(secret, decrypted);
    }
}