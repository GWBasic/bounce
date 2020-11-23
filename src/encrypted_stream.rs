// Currently commented out
// The right way to do this is to wrap an object that implements async Read and Write
// But I can't figure out the magic syntax to correctly wrap something that does Read and Write
// These are the frustrations that come with learning Rust


/*
use async_std::io;
use async_std::io::{Read, Result, Write};
use async_std::task::{Context, Poll};
use std::marker::Unpin;
use std::pin::Pin;

use rand_core::{CryptoRng, RngCore};

pub struct EncryptedStream<TStream, TRng> where
    TStream: Read + Write,
    TRng: CryptoRng + RngCore {
    wrapped_stream: TStream,
    write_xor: Xor<TRng>,
    read_xor: Xor<TRng>,
}

pub struct Xor<TRng> {
    rng: TRng,
    xor: [u8; 1024],
    ctr: usize,
}

impl<TStream, TRng> Unpin for EncryptedStream<TStream, TRng> where
TStream: Read + Write,
TRng: CryptoRng + RngCore {}

impl<TStream, TRng> EncryptedStream<TStream, TRng> where 
    TStream: Read + Write,
    TRng: CryptoRng + RngCore {
    pub fn new(wrapped_stream: TStream, write_rng: TRng, read_rng: TRng) -> EncryptedStream<TStream, TRng> {
        EncryptedStream {
            wrapped_stream,
            write_xor: Xor::new(write_rng),
            read_xor: Xor::new(read_rng),
        }
    }
}

impl<TRng> Xor<TRng> where
    TRng: CryptoRng + RngCore {

    fn new(rng: TRng) -> Xor<TRng> {
        Xor {
            rng,
            xor: [0u8; 1024],
            ctr: usize::MAX,
        }
    }

    fn next_byte(&mut self) -> u8 {
        if self.ctr >= self.xor.len() {
            self.rng.fill_bytes(&mut self.xor[..]);
            self.ctr = 0;
        }

        let b = self.xor[self.ctr];
        self.ctr = self.ctr + 1;
        b
    }
}

impl<TStream: Read + Unpin, TRng> Read for EncryptedStream<TStream, TRng> where
    TStream: Read + Write,
    TRng: CryptoRng + RngCore {

    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.wrapped_stream).poll_read(cx, buf);

        match result {
            Poll::Ready(result) => match result {
                Result::Ok(size) => {
                    for ctr in 0..size {
                        buf[ctr] = buf[ctr] ^ self.read_xor.next_byte();
                    }
        
                    Poll::Ready(Result::Ok(size))
                },
                Result::Err(err) => Poll::Ready(Result::Err(err))
            },
            Poll::Pending => Poll::Pending
        }
    }
}

impl<TStream: Write + Unpin, TRng> Write for EncryptedStream<TStream, TRng> where
    TStream: Read + Write,
    TRng: CryptoRng + RngCore {

    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut encrypted = vec![0; buf.len()];

        for ctr in 0..buf.len() {
            encrypted[ctr] = buf[ctr] ^ self.write_xor.next_byte();
        }

        Pin::new(&mut self.wrapped_stream).poll_write(cx, &encrypted)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.wrapped_stream).poll_flush(cx)
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.wrapped_stream).poll_close(cx)
    }
}


// Test with https://docs.rs/async-std/1.7.0/async_std/io/struct.Cursor.html
#[cfg(test)]
mod tests {
    use async_std::io::Cursor;

    use futures::io::{AsyncReadExt, AsyncWriteExt};
    use rand::{Rng, SeedableRng, thread_rng};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    fn create_stream_and_data_and_rng(stream_buf: Vec<u8>) -> (Cursor<Vec<u8>>, <ChaCha8Rng as SeedableRng>::Seed, ChaCha8Rng, ChaCha8Rng) {
        let memory_stream = Cursor::new(stream_buf);

        let mut test_seed: <ChaCha8Rng as SeedableRng>::Seed = Default::default();
        thread_rng().fill(&mut test_seed);
        let test_rng = ChaCha8Rng::from_seed(test_seed.clone());

        let ignored_rng = ChaCha8Rng::seed_from_u64(0);

        (memory_stream, test_seed, test_rng, ignored_rng)
    }

    #[async_std::test]
    async fn encrypted_stream_works_write() {
        let len = 1024 * 1024;
        let stream_buf = vec![0u8; len];

        let (memory_stream, test_seed, test_rng, ignored_rng) = create_stream_and_data_and_rng(stream_buf);

        let mut test_contents = vec![0u8; len];
        thread_rng().fill(&mut test_contents[..]);

        let mut encrypted_stream = EncryptedStream::new(memory_stream, test_rng, ignored_rng);

        encrypted_stream.write_all(&test_contents).await.unwrap();

        let stream_buf_encrypted = encrypted_stream.wrapped_stream.into_inner();

        // Verify that the contents changed
        assert_ne!(test_contents, stream_buf_encrypted, "Contents weren't encrypted");

        // Verify each byte
        let test_rng = ChaCha8Rng::from_seed(test_seed.clone());
        let mut xor = Xor::new(test_rng);
        for ctr in 0..test_contents.len() {
            let b = xor.next_byte();
            assert_eq!(test_contents[ctr], stream_buf_encrypted[ctr] ^ b, "Encrypted content isn't as expected");
        }
    }

    #[async_std::test]
    async fn encrypted_stream_works_read() {
        let len = 1024 * 1024;
        let mut encrypted_contents = vec![0u8; len];
        thread_rng().fill(&mut encrypted_contents[..]);

        let (memory_stream, test_seed, test_rng, ignored_rng) = create_stream_and_data_and_rng(encrypted_contents.clone());

        let mut encrypted_stream = EncryptedStream::new(memory_stream, ignored_rng, test_rng);

        let mut decrypted_contents = vec![0u8; encrypted_contents.len()];
        let mut bytes_read = 0;
        loop {
            bytes_read = bytes_read + encrypted_stream.read(&mut decrypted_contents[bytes_read..]).await.unwrap();

            if bytes_read >= decrypted_contents.len() {
                break;
            }
        }

        // Verify that the contents changed
        assert_ne!(encrypted_contents, decrypted_contents, "Contents weren't decrypted");

        // Verify each byte
        let test_rng = ChaCha8Rng::from_seed(test_seed.clone());
        let mut xor = Xor::new(test_rng);
        for ctr in 0..encrypted_contents.len() {
            let b = xor.next_byte();
            assert_eq!(decrypted_contents[ctr], encrypted_contents[ctr] ^ b, "Decrypted content isn't as expected");
        }
    }
}
*/