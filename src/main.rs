mod auth;
mod bridge;
mod client;
mod completion_token;
mod keys;
mod server;
mod xor;

use std::env::{args, var};
use std::io::{ Error, ErrorKind, Write };

use chrono::Local;
use env_logger::Builder;
use log::LevelFilter;

use client::run_client;
use completion_token::CompletionToken;
use keys::{Key, generate_keys, parse_key};
use server::run_server;

#[async_std::main]
async fn main() {
    println!("Bounce");

    setup_logging();

    let result = match var("BOUNCE_MODE") {
        Ok(mode) => main_env(mode).await,
        Err(_) => main_args().await
    };

    match result {
        Ok(()) => {},
        Err(err) => log::error!("Bounce terminated in error:\n\t{}", err)
    }
}

fn setup_logging() {
    Builder::new()
        .parse_env("BOUNCE_LOG")
        .format(|buf, record| {
            writeln!(buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();
}

async fn main_env(mode: String) -> Result<(), Error> {

    // Environment errors are logged, because it's assumed these need to go into a standard logger

    match parse_mode(&mode) {
        Mode::Server => {
            let port = get_port_from_env("BOUNCE_PORT")?;
            let adapter_port = get_port_from_env("BOUNCE_ADAPTER_PORT")?;
            let key = get_key_from_env("BOUNCE_KEY")?;
        
            run_server(port, adapter_port, key, CompletionToken::new()).await?;
        },
        Mode::Client => {
            let bounce_server = get_env_var("BOUNCE_SERVER")?;
            let destination_host = get_env_var("BOUNCE_DESTINATION_HOST")?;
            let key = get_key_from_env("BOUNCE_KEY")?;

            run_client(bounce_server, destination_host, key).await?;
        },
        Mode::Keys => {
            generate_keys();
        }
    }

    Ok(())
}

async fn main_args() -> Result<(), Error> {
    let args: Vec<String> = args().collect();

    // Panics are used instead of logging because it's assumed that bounce is being run interactively

    if args.len() < 2 {
        panic!("Must pass the mode (Server or Client) as the first argument");
    }

    match parse_mode(&args[1]) {
        Mode::Server => {
            if args.len() != 5 {
                panic!("Please specify the ports as command-line arguments:\n\t bounce server [port] [adapter port] [key]");
            }
        
            let port = parse_port(&args[2]).unwrap();
            let adapter_port = parse_port(&args[3]).unwrap();
            let key = parse_key(&args[4]);
        
            run_server(port, adapter_port, key, CompletionToken::new()).await?;
        },
        Mode::Client => {

            if args.len() != 5 {
                panic!("Please specify the host and port as command-line arguments:\n\t bounce client [bounce server:port] [destination:port] [key]");
            }
        
            let bounce_server = args[2].clone();
            let destination_host = args[3].clone();
            let key = parse_key(&args[4]);
        
            run_client(bounce_server, destination_host, key).await?;
        },
        Mode::Keys => {
            if args.len() != 2 {
                panic!("Generating keys takes no arguments");
            }

            generate_keys();
        }
    }

    Ok(())
}

fn get_env_var(var_name: &str) -> Result<String, Error> {
    match var(var_name) {
        Ok(val) => Ok(val),
        Err(_) => Err(Error::new(ErrorKind::Other, format!("{} must be set", var_name)))
    }
}

fn get_port_from_env(var_name: &str) -> Result<u16, Error> {
    let port_str = get_env_var(var_name)?;
    Ok(parse_port(&port_str)?)
}

fn parse_port(port_str: &str) -> Result<u16, Error> {
    match port_str.parse::<u16>() {
        Ok(port) => Ok(port),
        Err(err) => Err(Error::new(ErrorKind::Other, format!("Invalid port \"{}\": {}", port_str, err)))
    }
}

fn get_key_from_env(var_name: &str) -> Result<Key, Error> {
    let key_str = get_env_var(var_name)?;
    Ok(parse_key(&key_str))
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
