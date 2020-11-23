use rand_core::{CryptoRng, RngCore};

const XOR_BUFFER_SIZE: usize = 1024;

#[derive(Copy)]
#[derive(Clone)]
pub struct Xor<TRng> where
TRng: CryptoRng + RngCore + Clone {
    rng: TRng,
    xor: [u8; 1024],
    ctr: usize,
}

#[derive(Copy)]
#[derive(Clone)]
pub struct Xors<TRng> where
TRng: CryptoRng + RngCore + Clone {
    pub write_xor: Xor<TRng>,
    pub read_xor: Xor<TRng>
}

unsafe impl<TRng> Send for Xor<TRng>  where
TRng: CryptoRng + RngCore + Clone  {
}

impl<TRng> Xor<TRng> where
TRng: CryptoRng + RngCore + Clone  {

    pub fn new(rng: TRng) -> Xor<TRng> {
        Xor {
            rng,
            xor: [0u8; XOR_BUFFER_SIZE],
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

    pub fn process(&mut self, data: &mut [u8]) {
        for ctr in 0..data.len() {
            let b = data[ctr] ^ self.next_byte();
            data[ctr] = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::{RngCore, SeedableRng, thread_rng};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn same_seed_generates_same_sequence() {
        let mut xor = Xor::new(ChaCha8Rng::seed_from_u64(1));
        let mut rng = ChaCha8Rng::seed_from_u64(1);

        for _ in 0..5 {
            let mut buf = vec![0u8; XOR_BUFFER_SIZE];
            rng.fill_bytes(&mut buf);

            for ctr in 0..XOR_BUFFER_SIZE {
                assert_eq!(buf[ctr], xor.next_byte());
            }
        }
    }

    #[test]
    fn process() {
        let mut xor = Xor::new(ChaCha8Rng::seed_from_u64(1));
        let mut rng = ChaCha8Rng::seed_from_u64(1);

        for _ in 0..5 {
            let mut xor_buf = vec![0u8; XOR_BUFFER_SIZE];
            rng.fill_bytes(&mut xor_buf);

            let mut buf_for_test = vec![0u8; XOR_BUFFER_SIZE];
            thread_rng().fill_bytes(&mut buf_for_test);

            let mut buf_for_xor = buf_for_test.clone();

            for ctr in 0..XOR_BUFFER_SIZE {
                buf_for_test[ctr] = buf_for_test[ctr] ^ xor_buf[ctr];
            }

            xor.process(&mut buf_for_xor);

            assert_eq!(buf_for_test, buf_for_xor);
        }
    }
}