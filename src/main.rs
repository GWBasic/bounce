mod bridge;
mod client;
mod keys;
mod server;

use std::env::{args, var};

use client::run_client;
use keys::{generate_keys, parse_key};
use server::run_server;

#[async_std::main]
async fn main() {
    println!("Bounce");

    match var("BOUNCE_MODE") {
        Ok(mode) => main_env(mode).await,
        Err(_) => main_args().await
    }
}

async fn main_env(mode: String) {
    match parse_mode(&mode) {
        Mode::Server => {
            let port = get_port_from_env("BOUNCE_PORT");
            let adapter_port = get_port_from_env("BOUNCE_ADAPTER_PORT");
            let key = get_key_from_env("BOUNCE_KEY");
        
            run_server(port, adapter_port, key).await;
        },
        Mode::Client => {
            let bounce_server = get_server_from_env("BOUNCE_SERVER");
            let destination_host = get_server_from_env("BOUNCE_DESTINATION_HOST");
            let key = get_key_from_env("BOUNCE_KEY");

            run_client(bounce_server, destination_host, key).await;
        },
        Mode::Keys => {
            generate_keys();
        }
    }
}

async fn main_args() {
    let args: Vec<String> = args().collect();

    if args.len() < 2 {
        panic!("Must pass the mode (Server or Client) as the first argument");
    }

    match parse_mode(&args[1]) {
        Mode::Server => {
            if args.len() != 5 {
                panic!("Please specify the ports as command-line arguments:\n\t bounce server [port] [adapter port] [key]");
            }
        
            let port = parse_port(&args[2]);
            let adapter_port = parse_port(&args[3]);
            let key = parse_key(&args[4]);
        
            run_server(port, adapter_port, key).await;

        },
        Mode::Client => {

            if args.len() != 5 {
                panic!("Please specify the host and port as command-line arguments:\n\t bounce client [bounce server:port] [destination:port] [key]");
            }
        
            let bounce_server = args[2].clone();
            let destination_host = args[3].clone();
            let key = parse_key(&args[4]);
        
            run_client(bounce_server, destination_host, key).await;
        },
        Mode::Keys => {
            generate_keys();
        }
    }
}

fn get_port_from_env(var_name: &str) -> u16 {
    match var(var_name) {
        Ok(port_str) => parse_port(&port_str),
        Err(_) => panic!("{} must be set", var_name)
    }
}

fn parse_port(port_string: &str) -> u16 {
    match port_string.parse::<u16>() {
        Ok(port) => port,
        Err(err) => panic!("Invalid port \"{}\": {}", port_string, err)
    }
}

fn get_key_from_env(var_name: &str) -> [u8; 16] {
    match var(var_name) {
        Ok(key_str) => parse_key(&key_str),
        Err(_) => panic!("{} must be set", var_name)
    }
}

fn get_server_from_env(var_name: &str) -> String {
    match var(var_name) {
        Ok(server) => server,
        Err(_) => panic!("{} must be set", var_name)
    }
}

enum Mode {
    Server,
    Client,
    Keys
}

fn parse_mode(mode: &String) -> Mode {
    if mode == "server" {
        Mode::Server
    } else if mode == "client" {
        Mode::Client
    } else if mode == "keys" {
        Mode::Keys
    } else {
        panic!("Unknown mode: {}", mode);
    }
}