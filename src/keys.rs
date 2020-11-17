extern crate rand;

use rand::RngCore;
use rand::rngs::OsRng;
use rustc_serialize::base64::{STANDARD, ToBase64};

pub fn generate_keys() {
    let mut key = [0u8; 16];
    OsRng.fill_bytes(&mut key);

    println!("Key: {}", key.to_base64(STANDARD));
}