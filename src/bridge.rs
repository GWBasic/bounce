use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use async_std::task;
use std::io::{Error, ErrorKind};

use crypto::aes;
use futures::future::{Either, join, select};

use crate::keys::{Key, Nonces};

pub fn run_bridge(key: Key, nonces: Nonces, clear_stream: TcpStream, clear_stream_name: String, encrypted_stream: TcpStream, encrypted_stream_name: String) {

    match clear_stream.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", clear_stream_name, err);
            return;
        },
        Ok(()) => {}
    }

    match encrypted_stream.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", encrypted_stream_name, err);
            return;
        },
        Ok(()) => {}
    }

    task::spawn(bridge(key, nonces, clear_stream, clear_stream_name, encrypted_stream, encrypted_stream_name));
}

pub async fn bridge(key: Key, nonces: Nonces, clear_stream: TcpStream, clear_stream_name: String, encrypted_stream: TcpStream, encrypted_stream_name: String) {

    let write_future = task::spawn(bridge_connections_encrypted_write(
        key.clone(),
        nonces.my_nonce,
        clear_stream.clone(),
        clear_stream_name.clone(),
        encrypted_stream.clone(),
        encrypted_stream_name.clone()));

    let read_future = task::spawn(bridge_connections_encrypted_read(
        key,
        nonces.their_nonce,
        encrypted_stream.clone(),
        encrypted_stream_name.clone(),
        clear_stream.clone(),
        clear_stream_name.clone()));

    match select(write_future, read_future).await {
        Either::Left(r) => match r.0 {
            Err(err) => println!("{} -> {} ended: {}", clear_stream_name, encrypted_stream_name, err),
            _ => {}
        },
        Either::Right(r) => match r.0 {
            Err(err) => println!("{} -> {} ended: {}", encrypted_stream_name, clear_stream_name, err),
            _ => {}
        },
    }

    let clear_flush_future = task::spawn(flush(clear_stream.clone(), clear_stream_name.clone()));
    let encrypted_flush_future = task::spawn(flush(encrypted_stream.clone(), encrypted_stream_name.clone()));

    join(clear_flush_future, encrypted_flush_future).await;

    println!("Connection ended");

    match clear_stream.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", clear_stream_name),
        Err(err) => println!("Error shutting down {}: {}", clear_stream_name, err)
    }

    match encrypted_stream.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", encrypted_stream_name),
        Err(err) => println!("Error shutting down {}: {}", encrypted_stream_name, err)
    }
}

async fn bridge_connections_encrypted_read(key: Key, nonce: Vec<u8>, mut reader: TcpStream, reader_name: String, mut writer: TcpStream, _writer_name: String) -> Result<(), Error> {
    
    let keysize = key.key.len();
    let mut buf_encrypted = vec![0u8; keysize];
    let mut buf_decrypted = vec![0u8; keysize];

    loop {
        // Reads come in chunks of keysize
        let mut bytes_read_in_packet: usize = 0;
        'read_loop: loop {
            let bytes_read = reader.read(&mut buf_encrypted[bytes_read_in_packet..keysize]).await?;

            if bytes_read == 0 {
                if bytes_read_in_packet == 0 {
                    return Ok(());
                } else {
                    return Err(Error::new(ErrorKind::Interrupted, format!("{} terminated with incomplete packet", reader_name)));
                }
            }

            bytes_read_in_packet = bytes_read_in_packet + bytes_read;

            if bytes_read_in_packet >= keysize {
                break 'read_loop;
            }
        }

        // Decrypt
        process(&key, &nonce, &buf_encrypted, &mut buf_decrypted);

        // Note: An assumption is that there will never be more than a 256 bit key, thus the size of the buffer will always fit into the first byte
        let packet_size: usize = buf_decrypted[0] as usize;
        //println!("Read {} bytes from {}", packet_size, reader_name);

        let write_slice = &buf_decrypted[1..packet_size + 1];
        writer.write_all(write_slice).await?;
    }
}

async fn bridge_connections_encrypted_write(key: Key, nonce: Vec<u8>, mut reader: TcpStream, _reader_name: String, mut writer: TcpStream, _writer_name: String) -> Result<(), Error> {
    
    let keysize = key.key.len();
    let mut buf_clear = vec![0u8; keysize];
    let mut buf_encrypted = vec![0u8; keysize];

    loop {
        let packet_size = reader.read(&mut buf_clear[1..]).await?;

        if packet_size == 0 {
            //println!("{} complete", reader_name);
            return Ok(());
        }

        //println!("Read {} bytes from {}", packet_size, reader_name);

        // Note: An assumption is that there will never be more than a 256 bit key, thus the size of the buffer will always fit into the first byte
        buf_clear[0] = packet_size as u8;

        // Encrypt
        process(&key, &nonce, &buf_clear, &mut buf_encrypted);

        writer.write_all(&buf_encrypted[..]).await?;
        //println!("Wrote {} bytes to {}", packet_size, writer_name);
    }
}

fn process(key: &Key, nonce: &Vec<u8>, source: &Vec<u8>, destination: &mut [u8]) {
    let mut ciper = aes::ctr(key.size, &key.key, &nonce);
    ciper.process(source, destination);
}

async fn flush(mut stream: TcpStream, stream_name: String) {
    match stream.flush().await {
        Err(err) => println!("Can not flush {}: {}", stream_name, err),
        Ok(()) => {}
    }
}

#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
    use async_std::prelude::*;

    use crypto::aes::KeySize;
    use rand::{RngCore, thread_rng};

    use super::*;

    struct TcpStreams {
        initiating_client_clear_stream: TcpStream,
        bounce_server_clear_stream: TcpStream,
        bounce_server_encrypted_stream: TcpStream,
        bounce_client_encrypted_stream: TcpStream,
        bounce_client_clear_stream: TcpStream,
        final_client_clear_stream: TcpStream
    }

    impl Drop for TcpStreams {
        fn drop(&mut self) {
            self.initiating_client_clear_stream.shutdown(Shutdown::Both).ok();
            self.bounce_server_clear_stream.shutdown(Shutdown::Both).ok();
            self.bounce_server_encrypted_stream.shutdown(Shutdown::Both).ok();
            self.bounce_client_encrypted_stream.shutdown(Shutdown::Both).ok();
            self.bounce_client_clear_stream.shutdown(Shutdown::Both).ok();
            self.final_client_clear_stream.shutdown(Shutdown::Both).ok();
        }
    }

    async fn get_socket_streams() -> TcpStreams {

        // socket going in -> clear
        // socket between server and client -> encrypted
        // socket going out -> clear


        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let listener = TcpListener::bind(socket_addr).await.unwrap();

        let local_addr = listener.local_addr().unwrap();

        let initiating_client_clear_stream = TcpStream::connect(local_addr).await.unwrap();
        let bounce_server_clear_stream = listener.incoming().next().await.unwrap().unwrap();

        let bounce_server_encrypted_stream = TcpStream::connect(local_addr).await.unwrap();
        let bounce_client_encrypted_stream = listener.incoming().next().await.unwrap().unwrap();

        let bounce_client_clear_stream = TcpStream::connect(local_addr).await.unwrap();
        let final_client_clear_stream = listener.incoming().next().await.unwrap().unwrap();

        TcpStreams {
            initiating_client_clear_stream,
            bounce_server_clear_stream,
            bounce_server_encrypted_stream,
            bounce_client_encrypted_stream,
            bounce_client_clear_stream,
            final_client_clear_stream
        }
    }

    async fn start() -> TcpStreams {
        let key = Key {
            key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            size: KeySize::KeySize128
        };

        let streams = get_socket_streams().await;

        let nonces_server = Nonces {
            my_nonce: vec![17 as u8, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
            their_nonce: vec![33 as u8, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48]
        };

        let nonces_client = Nonces {
            my_nonce: nonces_server.their_nonce.clone(),
            their_nonce: nonces_server.my_nonce.clone()
        };

        // server
        run_bridge(
            key.clone(),
            nonces_server,
            streams.bounce_server_clear_stream.clone(),
            "bounce_server_clear_stream".to_string(),
            streams.bounce_server_encrypted_stream.clone(),
            "bounce_server_encrypted_stream".to_string());

        // client
        run_bridge(
            key.clone(),
            nonces_client,
            streams.bounce_client_clear_stream.clone(),
            "bounce_client_clear_stream".to_string(),
            streams.bounce_client_encrypted_stream.clone(),
            "bounce_client_encrypted_stream".to_string());

        streams
    }

    #[async_std::test]
    async fn bridge_works() {
        let streams = start().await;

        let mut write_stream = &streams.initiating_client_clear_stream;
        let mut read_stream = &streams.final_client_clear_stream;

        for _ in 0usize..256 {
            let mut rng = thread_rng();

            let size = (rng.next_u64() % 4098) as usize;
            let mut send_buf = vec![0u8; size];
            rng.fill_bytes(&mut send_buf);

            write_stream.write_all(&send_buf[..]).await.expect("Can not write to initiating_client_clear_stream");

            let mut recieve_buf = vec![0u8; size];
            let mut total_bytes_read = 0usize;

            while total_bytes_read < size {
                let bytes_read = read_stream.read(&mut recieve_buf[total_bytes_read..]).await.expect("Can not read from final_client_clear_stream");
                assert_ne!(bytes_read, 0, "Unexpected end of stream");
                total_bytes_read = total_bytes_read + bytes_read;
            }

            assert_eq!(send_buf, recieve_buf, "Wrong contents sent");

            // Exchange
            let i = write_stream;
            write_stream = read_stream;
            read_stream = i;
        }
    }

    async fn shutdown_read(write_stream: &TcpStream, read_stream: &mut TcpStream) {
        write_stream.shutdown(Shutdown::Both).unwrap();

        let mut read_buf = vec![0u8, 16];
        let bytes_read = read_stream.read(&mut read_buf[..]).await.unwrap();

        assert_eq!(bytes_read, 0, "Socket should be shut down");
    }

    #[async_std::test]
    async fn shutdown_incoming_read() {
        let mut streams = start().await;
        shutdown_read(&streams.initiating_client_clear_stream, &mut streams.final_client_clear_stream).await;
    }

    #[async_std::test]
    async fn shutdown_outgoing_read() {
        let mut streams = start().await;
        shutdown_read(&streams.final_client_clear_stream, &mut streams.initiating_client_clear_stream).await;
    }
}