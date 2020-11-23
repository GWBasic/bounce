extern crate rand;

use crypto::aes::KeySize;
use rand::RngCore;
use rand::rngs::OsRng;
use rustc_serialize::base64::{FromBase64, STANDARD, ToBase64};

#[derive(Clone)]
pub struct Key {
    pub key: Vec<u8>,
    // This is always KeySize::KeySize256
    pub size: KeySize
}

pub fn generate_keys() {
    let mut key = vec![0u8; 256 / 8];
    OsRng.fill_bytes(&mut key);

    println!("Key: {}", key.to_base64(STANDARD));
}

pub fn parse_key(key_str: &str) -> Key {
    let key = match key_str.from_base64() {
        Err(err) => panic!("Can not parse key {}: {}", key_str, err),
        Ok(v) => v
    };

    if key.len() != 256 / 8 {
        panic!("Only 256-bit keys supported")
    }

    let size = KeySize::KeySize256;

    Key {key, size}
}

#[cfg(test)]
mod tests {
    use super::*;

    use rustc_serialize::base64::STANDARD;

    // https://stackoverflow.com/a/25577080/1711103
    macro_rules! matches(
        ($expected:pat, $actual:expr) => (
            match $actual {
                $expected => (),
                _ => panic!("Unexpected value")
            }
        )
    );

    #[test]
    fn parse_key_test_256() {
        let key = vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32];
        let key_str = key.to_base64(STANDARD);

        let parsed_key = parse_key(&key_str);
        assert_eq!(key, parsed_key.key);
        matches!(KeySize::KeySize256, parsed_key.size);
    }
}