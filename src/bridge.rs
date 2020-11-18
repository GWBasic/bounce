use async_std::net::{Shutdown, TcpStream};
use async_std::prelude::*;
use async_std::task;

pub fn bridge(a: TcpStream, a_name: String, b: TcpStream, b_name: String) {

    match a.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", a_name, err);
            return;
        },
        Ok(()) => {}
    }

    match b.set_nodelay(true) {
        Err(err) => {
            println!("Error disabling Nagle on {}: {}", b_name, err);
            return;
        },
        Ok(()) => {}
    }

    task::spawn(bridge_connections(a.clone(), a_name.clone(), b.clone(), b_name.clone()));
    task::spawn(bridge_connections(b, b_name, a, a_name));

    // TODO: Await and log
}

async fn bridge_connections(mut reader: TcpStream, reader_name: String, mut writer: TcpStream, writer_name: String)  {
    
    let mut buf: [u8; 4096] = [0; 4096];

    'bridge: loop {
        match reader.read(&mut buf).await {
            Err(err) => {
                println!("Reading {} stopped: {}", reader_name, err);                
                break 'bridge;
            },
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    println!("{} complete", reader_name);
                    break 'bridge;
                }

                println!("Read {} bytes from {}", bytes_read, reader_name);

                let write_slice = &buf[0..bytes_read];
                match writer.write_all(write_slice).await {
                    Err(err) => {
                        println!("Writing {} stopped: {}", writer_name, err);
                        break 'bridge;
                    },
                    Ok(()) => {
                        println!("Wrote {} bytes to {}", bytes_read, writer_name);
                    }
                }
            }
        }
    }

    match writer.flush().await {
        Err(err) => {
            println!("Can not flush: {}", err);
        },
        Ok(()) =>{}
    }

    match reader.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", reader_name),
        Err(err) => println!("Error shutting down {}: {}", reader_name, err)
    }

    match writer.shutdown(Shutdown::Both) {
        Ok(()) => println!("Successfully shut down {}", writer_name),
        Err(err) => println!("Error shutting down {}: {}", writer_name, err)
    }
}
