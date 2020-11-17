extern crate rand;

//use std::convert::TryInto;

use crypto::aes::KeySize;
use rand::RngCore;
use rand::rngs::OsRng;
use rustc_serialize::base64::{FromBase64, STANDARD, ToBase64};

pub struct Key {
    pub key: Vec<u8>,
    pub size: KeySize
}

pub fn generate_keys(size_bits: usize) {

    // Filter key sizes
    let _ = get_key_size(size_bits);

    let mut key = vec![0u8; size_bits / 8];
    OsRng.fill_bytes(&mut key);

    println!("Key: {}", key.to_base64(STANDARD));
}

pub fn parse_key(key_str: &str) -> Key {
    let key = match key_str.from_base64() {
        Err(err) => panic!("Can not parse key {}: {}", key_str, err),
        Ok(v) => v
    };

    let size = get_key_size(key.len() * 8);

    Key {key, size}

    /*

    if key_vec.len() != 16 {
        panic!("Expected key length of 16, actual key length: {}", key_vec.len())
    }

    // https://stackoverflow.com/a/29570662/1711103
    let key_slice = key_vec.into_boxed_slice();

    let key: Box<[u8; 16]> = match key_slice.try_into() {
        Err(_) => panic!("Can not convert key vector to array"),
        Ok(k) => k,
    };

    *key*/
}

pub fn get_key_size(size_bits: usize) -> KeySize {
    match size_bits {
        128 => KeySize::KeySize128,
        192 => KeySize::KeySize192,
        256 => KeySize::KeySize256,
        _ => panic!("Keysize {} not supported", size_bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    //use rustc_serialize::base64::{STANDARD, ToBase64};
    use rustc_serialize::base64::STANDARD;

    #[test]
    fn parse_key_test_128() {
        let key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let key_str = key.to_base64(STANDARD);

        let parsed_key = parse_key(&key_str);
        assert_eq!(key, parsed_key.key);
        //assert_eq!(KeySize::KeySize128, parsed_key.size);
    }

    #[test]
    fn parse_key_test_192() {
        let key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26];
        let key_str = key.to_base64(STANDARD);

        let parsed_key = parse_key(&key_str);
        assert_eq!(key, parsed_key.key);
        //assert_eq!(KeySize::KeySize192, parsed_key.size);
    }

    #[test]
    fn parse_key_test_256() {
        let key: vec![1 as u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32];
        let key_str = key.to_base64(STANDARD);

        let parsed_key = parse_key(&key_str);
        assert_eq!(key, parsed_key.key);
        //assert_eq!(KeySize::KeySize256, parsed_key.size);
    }
}