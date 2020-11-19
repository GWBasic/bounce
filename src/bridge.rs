use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use async_std::task;

use crypto::aes;
use futures::future::{join, select};

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

    select(write_future, read_future).await;

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

async fn bridge_connections_encrypted_read(key: Key, nonce: Vec<u8>, mut reader: TcpStream, reader_name: String, mut writer: TcpStream, writer_name: String)  {
    
    let keysize = key.key.len();
    let mut buf_encrypted = vec![0u8; keysize];
    let mut buf_decrypted = vec![0u8; keysize];

    loop {
        // Reads come in chunks of keysize
        let mut bytes_read_in_packet: usize = 0;
        'read_loop: loop {
            let bytes_read = match reader.read(&mut buf_encrypted).await {
                Err(err) => {
                    println!("Reading {} stopped: {}", reader_name, err);                
                    return;
                },
                Ok(bytes_read) => {
                    if bytes_read == 0 {
                        if bytes_read_in_packet == 0 {
                            println!("{} complete", reader_name);
                        } else {
                            println!("{} terminated with incomplete packet", reader_name);
                        }

                        return;
                    }

                    bytes_read
                }
            };

            bytes_read_in_packet = bytes_read_in_packet + bytes_read;

            if bytes_read_in_packet >= keysize {
                break 'read_loop;
            }
        }

        // Decrypt
        process(&key, &nonce, &buf_encrypted, &mut buf_decrypted);

        // Note: An assumption is that there will never be more than a 256 bit key, thus the size of the buffer will always fit into the first byte
        let packet_size: usize = buf_decrypted[0] as usize;
        println!("Read {} bytes from {}", packet_size, reader_name);

        let write_slice = &buf_decrypted[1..packet_size + 1];
        match writer.write_all(write_slice).await {
            Err(err) => {
                println!("Writing {} stopped: {}", writer_name, err);
                return;
            },
            Ok(()) => {
                println!("Wrote {} bytes to {}", packet_size, writer_name);
            }
        }
    }
}

async fn bridge_connections_encrypted_write(key: Key, nonce: Vec<u8>, mut reader: TcpStream, reader_name: String, mut writer: TcpStream, writer_name: String)  {
    
    let keysize = key.key.len();
    let mut buf_clear = vec![0u8; keysize];
    let mut buf_encrypted = vec![0u8; keysize];

    loop {
        let packet_size = match reader.read(&mut buf_clear[1..]).await {
            Err(err) => {
                println!("Reading {} stopped: {}", reader_name, err);                
                return;
            },
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    println!("{} complete", reader_name);
                    return;
                }

                bytes_read
            }
        };

        println!("Read {} bytes from {}", packet_size, reader_name);

        // Note: An assumption is that there will never be more than a 256 bit key, thus the size of the buffer will always fit into the first byte
        buf_clear[0] = packet_size as u8;

        // Encrypt
        process(&key, &nonce, &buf_clear, &mut buf_encrypted);

        match writer.write_all(&buf_encrypted[..]).await {
            Err(err) => {
                println!("Writing {} stopped: {}", writer_name, err);
                return;
            },
            Ok(()) => {
                println!("Wrote {} bytes to {}", packet_size, writer_name);
            }
        }
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

