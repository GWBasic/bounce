extern crate rand;

use std::convert::TryInto;

use rand::RngCore;
use rand::rngs::OsRng;
use rustc_serialize::base64::{FromBase64, STANDARD, ToBase64};

pub fn generate_keys() {
    let mut key = [0u8; 16];
    OsRng.fill_bytes(&mut key);

    println!("Key: {}", key.to_base64(STANDARD));
}

pub fn parse_key(key_str: &str) -> [u8; 16] {
    let key_vec = match key_str.from_base64() {
        Err(err) => panic!("Can not parse key {}: {}", key_str, err),
        Ok(v) => v
    };

    if key_vec.len() != 16 {
        panic!("Expected key length of 16, actual key length: {}", key_vec.len())
    }

    // https://stackoverflow.com/a/29570662/1711103
    let key_slice = key_vec.into_boxed_slice();

    let key: Box<[u8; 16]> = match key_slice.try_into() {
        Err(_) => panic!("Can not convert key vector to array"),
        Ok(k) => k,
    };

    *key
}

#[cfg(test)]
mod tests {
    use super::*;

    use rustc_serialize::base64::{STANDARD, ToBase64};

    #[test]
    fn parse_key_test() {
        let key: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let key_str = key.to_base64(STANDARD);

        let parsed_key = parse_key(&key_str);
        assert_eq!(key, parsed_key);
    }
}