use std::net::{TcpListener, TcpStream};
use std::thread::{spawn, sleep};
use std::time::Duration;

use figment::{Figment, providers::{Format, Yaml, Env}};

use serde::Deserialize;
use tungstenite::{
    accept_hdr,
    handshake::server::{Request, Response}, 
    WebSocket,
};

use serde_json::Value;
use std::fs;

use server::state::StateDelta;
use shared::{Operation, Transaction};

fn listen(websocket: &mut WebSocket<TcpStream>, current_state: &mut Operation) {
    match current_state {
        Operation::RequestState => {
            websocket.send(Operation::RequestState.into()).expect("Could not request state");

            let remote_state = websocket.read().unwrap(); // bloqueo hasta que se reciba algo

            if !remote_state.is_binary() {
                panic!("Invalid read!");
            }

            let message_data = remote_state.into_data();
            let remote_json: Value = serde_json::from_slice(message_data.as_slice()).expect("Could not deserialize");

            let local_state = fs::read("test.json").expect("Could not open local state");
            let local_json: Value = serde_json::from_slice(local_state.as_slice()).expect("Could not deserialize");

            let state_delta = StateDelta::from_json(local_json, remote_json);

            println!("Missing keys local: {:?}", state_delta.not_in_local); // Should push delete
                                                                            // transaction
            println!("Missing keys remote: {:?}", state_delta.not_in_remote); // Should push add
                                                                              // transaction
            println!("Keys with different values: {:?}", state_delta.value_not_equal);

            *current_state = Operation::ExecuteTransaction(Transaction::Update(state_delta.value_not_equal[0].to_string())); // Testing only

            sleep(Duration::from_secs(1));
        },
        Operation::ExecuteTransaction(transaction) => {
            websocket.send(Operation::ExecuteTransaction(transaction.clone()).into()).unwrap();
            *current_state = Operation::RequestState;
        },
    }
}

#[derive(Deserialize)]
struct Config {
    port: Option<String>,
    address: Option<String>,
}

fn main() {
    env_logger::init();

    let config: Config = Figment::new()
        .merge(Yaml::file("config.yaml"))
        .join(Env::raw().only(&["PORT", "ADDRESS"]))
        .extract().unwrap();

    let address = config.address.unwrap_or("127.0.0.1".into());
    let port = config.port.unwrap_or("3000".into());
    
    let full_address = format!("{address}:{port}");
    println!("Listening at {full_address}");
    let server = TcpListener::bind(full_address).unwrap();

    for stream in server.incoming(){
        // Este move es para que el spawn (que hace uso de hebras)
        // sea dueño todo ese bloque (necesario por garantias de ciclo de vida)
        spawn(move || {
            let callback = |req: &Request, mut response: Response| { // esto es un lambda en Rust
                println!("handshake");
                println!("request path: {}", req.uri().path());

                for (header, _value) in req.headers() {
                    println!("* {header}");
                }

                let headers = response.headers_mut();

                headers.append("Authorization", "mi autorizacion".parse().unwrap());

                Ok(response)
            };

            let mut websocket = accept_hdr(stream.unwrap(), callback).unwrap();

            let mut current_state = Operation::RequestState;

            loop {
                listen(&mut websocket, &mut current_state);
            }
        });
    }
}
