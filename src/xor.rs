use rand_core::{CryptoRng, RngCore};

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

    pub fn process(&mut self, data: &mut [u8]) {
        for ctr in 0..data.len() {
            let b = data[ctr] ^ self.next_byte();
            data[ctr] = b;
        }
    }
}

// TODO: Tests
// - Should verify that the same seed generates the same sequence
// - Should verify xor-ing