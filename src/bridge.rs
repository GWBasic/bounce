use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use async_std::task;
use std::io::Error;

use core::any::Any;

use futures::future::{Either, join, select};
use rand_core::{CryptoRng, RngCore};

use crate::xor::{Xor, Xors};

pub fn run_bridge<TRng>(xors: Xors<TRng>, clear_stream: TcpStream, clear_stream_name: String, encrypted_stream: TcpStream, encrypted_stream_name: String) where
TRng: CryptoRng + RngCore + Clone + Any {

    match clear_stream.set_nodelay(true) {
        Err(err) => {
            log::error!("Error disabling Nagle on {}: {}", clear_stream_name, err);
            return;
        },
        Ok(()) => {}
    }

    match encrypted_stream.set_nodelay(true) {
        Err(err) => {
            log::error!("Error disabling Nagle on {}: {}", encrypted_stream_name, err);
            return;
        },
        Ok(()) => {}
    }

    task::spawn(bridge(xors.clone(), clear_stream, clear_stream_name, encrypted_stream, encrypted_stream_name));
}

pub async fn bridge<TRng>(xors: Xors<TRng>, clear_stream: TcpStream, clear_stream_name: String, encrypted_stream: TcpStream, encrypted_stream_name: String) where
TRng: CryptoRng + RngCore + Clone + Any {

    let write_future = task::spawn(run_bridge_loop(
        xors.write_xor,
        clear_stream.clone(),
        clear_stream_name.clone(),
        encrypted_stream.clone(),
        encrypted_stream_name.clone()));

    let read_future = task::spawn(run_bridge_loop(
        xors.read_xor,
        encrypted_stream.clone(),
        encrypted_stream_name.clone(),
        clear_stream.clone(),
        clear_stream_name.clone()));

    match select(write_future, read_future).await {
        Either::Left(r) => match r.0 {
            Ok(()) => {
                shutdown_both(clear_stream, clear_stream_name.clone(), Shutdown::Write, encrypted_stream, encrypted_stream_name.clone(), Shutdown::Both).await;
            },
            Err(err) => {
                shutdown_both(clear_stream, clear_stream_name.clone(), Shutdown::Both, encrypted_stream, encrypted_stream_name.clone(), Shutdown::Both).await;
                log::error!("{} -> {} ended in error: {}", clear_stream_name, encrypted_stream_name, err);
            }
        },
        Either::Right(r) => match r.0 {
            Ok(()) => {
                shutdown_both(encrypted_stream, encrypted_stream_name.clone(), Shutdown::Write, clear_stream, clear_stream_name.clone(), Shutdown::Both).await;
            },
            Err(err) => {
                shutdown_both(encrypted_stream, encrypted_stream_name.clone(), Shutdown::Both, clear_stream, clear_stream_name.clone(), Shutdown::Both).await;
                log::error!("{} -> {} ended in error: {}", encrypted_stream_name, clear_stream_name, err);
            }
        },
    };
}

async fn run_bridge_loop<TRng>(mut xor: Xor<TRng>, mut reader: TcpStream, reader_name: String, mut writer: TcpStream, writer_name: String) -> Result<(), Error>  where
TRng: CryptoRng + RngCore + Clone  {
    
    let mut buf = vec![0u8; 4098];

    loop {
        let bytes_read = reader.read(&mut buf).await?;

        if bytes_read == 0 {
            log::debug!("Connected ending: {}", reader_name);
            return Ok(());
        }

        log::trace!("Read {} bytes from {}", bytes_read, reader_name);

        // Decrypt
        xor.process(&mut buf[..bytes_read]);

        // Forward
        writer.write_all(&mut buf[..bytes_read]).await?;

        log::trace!("Wrote {} bytes to {}", bytes_read, writer_name);
    }
}

async fn shutdown_both(
    clear_stream: TcpStream,
    clear_stream_name: String,
    clear_stream_shutdown: Shutdown,
    encrypted_stream: TcpStream,
    encrypted_stream_name: String,
    encrypted_stream_shutdown: Shutdown) {
    
    let clear_flush_future = task::spawn(shutdown(clear_stream.clone(), clear_stream_name.clone(), clear_stream_shutdown));
    let encrypted_flush_future = task::spawn(shutdown(encrypted_stream.clone(), encrypted_stream_name.clone(), encrypted_stream_shutdown));

    join(clear_flush_future, encrypted_flush_future).await;

    log::info!("Connection ended: {} <-> {}", clear_stream_name, encrypted_stream_name);
}


async fn shutdown(
    mut stream: TcpStream,
    stream_name: String,
    shutdown: Shutdown) {

    match stream.flush().await {
        Ok(()) => log::debug!("Successfully flushed down {}", stream_name),
        Err(err) => log::error!("Can not flush {}: {}", stream_name, err),
    }

    match stream.shutdown(shutdown) {
        Ok(()) => log::debug!("Successfully shut down {}", stream_name),
        Err(err) => log::error!("Error shutting down {}: {}", stream_name, err)
    }
}

#[cfg(test)]
mod tests {
    use async_std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, SocketAddr};
    use async_std::prelude::*;

    use rand::{RngCore, SeedableRng, thread_rng};
    use rand_chacha::ChaCha8Rng;

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

        let streams = get_socket_streams().await;

        let xors = Xors {
            read_xor: Xor::new(ChaCha8Rng::seed_from_u64(1)),
            write_xor: Xor::new(ChaCha8Rng::seed_from_u64(2))
        };

        // server
        run_bridge(
            xors,
            streams.bounce_server_clear_stream.clone(),
            "bounce_server_clear_stream".to_string(),
            streams.bounce_server_encrypted_stream.clone(),
            "bounce_server_encrypted_stream".to_string());

        let xors = Xors {
            read_xor: Xor::new(ChaCha8Rng::seed_from_u64(2)),
            write_xor: Xor::new(ChaCha8Rng::seed_from_u64(1))
        };
    
        // client
        run_bridge(
            xors,
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
