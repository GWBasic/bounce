mod bridge;
mod client;
mod server;

use std::env::{args, var};

use client::run_client;
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
            let port = get_port_from_env("PORT");
            let adapter_port = get_port_from_env("ADAPTER_PORT");
        
            run_server(port, adapter_port).await;
        },
        Mode::Client => {
            let bounce_server = get_server_from_env("BOUNCE_SERVER");
            let destination_host = get_server_from_env("BOUNCE_DESTINATION_HOST");

            run_client(bounce_server, destination_host).await;
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
            if args.len() != 4 {
                panic!("Please specify the ports as command-line arguments:\n\t bounce server [port] [adapter port]");
            }
        
            let port = parse_port(&args[2]);
            let adapter_port = parse_port(&args[3]);
        
            run_server(port, adapter_port).await;

        },
        Mode::Client => {

            if args.len() != 4 {
                panic!("Please specify the host and port as command-line arguments:\n\t bounce client [bounce server:port] [destination:port]");
            }
        
            let bounce_server = args[2].clone();
            let destination_host = args[3].clone();
        
            run_client(bounce_server, destination_host).await;
        }
    }
}

fn get_port_from_env(var_name: &str) -> u16 {
    match var(var_name) {
        Ok(port_string) => parse_port(&port_string),
        Err(_) => panic!("{} must be set", var_name)
    }
}

fn parse_port(port_string: &str) -> u16 {
    match port_string.parse::<u16>() {
        Ok(port) => port,
        Err(err) => panic!("Invalid port \"{}\": {}", port_string, err)
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
    Client
}

fn parse_mode(mode: &String) -> Mode {
    if mode == "server" {
        Mode::Server
    } else if mode == "client" {
        Mode::Client
    } else {
        panic!("Unknown mode: {}", mode);
    }
}