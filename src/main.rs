mod signal_message;

use failure::{format_err, Error};
use tokio::signal;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use futures_util::stream::SplitSink;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, WebSocketStream, tungstenite::Message};
use crate::signal_message::SignalMessage;

type Result<T> = std::result::Result<T, Error>;

// struct Connection {
//     ice_candidates: Vec<String>,
// }

#[derive(Debug)]
struct Session {
    pub offer: Option<String>,
    pub caller_ice_candidates: Vec<String>,
    // Caller websocket?
    // Peer  websocket?
}

impl Session {
    pub fn new(offer: String) -> Self {
        Session {
            offer: Some(offer),
            caller_ice_candidates: Vec::new(),
        }
    }
}

struct SignalState {
    // UUID -> Session
    session_map: HashMap<String, Session>,
}

impl SignalState {
    pub fn new() -> Self {
        Self {
            session_map: HashMap::new()
        }
    }
}

async fn handle_message(signal_state: Arc<Mutex<SignalState>>, message: String, ws: &mut SplitSink<WebSocketStream<TcpStream>, Message>) -> Result<()> {
    let signal_message: SignalMessage = serde_json::from_str(message.as_str())?;

    match signal_message {
        SignalMessage::Offer { id, sdp } => {
            let mut current_state = signal_state.lock().await;

            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("Updating Session Id {} from old_offer: {:?} to new_offer: {:?}\n", &id, &session.offer, &sdp);
                session.offer = Some(sdp);
            } else {
                println!("Creating Session with Id {} and offer {}\n", &id, &sdp);
                current_state.session_map.insert(id, Session::new(sdp));
            }
        },
        SignalMessage::Answer { id, sdp } => {

        },
        SignalMessage::CallerIceCandidate { id, ice_candidate } => {
            let mut current_state = signal_state.lock().await;
            
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("ice candidate: {ice_candidate:?}\n");

                // let ice = serde_json::to_string(&ice_candidate).unwrap();

               // println!("Pushing ice_candidate {:?} to caller_ice_candidates for Session Id {}\n", &ice, &id);

                let ice_clone = ice_candidate.clone();

                session.caller_ice_candidates.push(ice_candidate);
                
                ws.send(Message::text(ice_clone.as_str())).await;
            } else {
                println!("A session should be created before adding ice_candidates? Look into this! It's possible we need to have an initial join/start message\n");
            } 
        },
        _ => {
            println!("Got here!");
        }
    }

    Ok(())
}

async fn handle_connection(signal_state: Arc<Mutex<SignalState>>, tcp_stream: TcpStream, addr: SocketAddr) {
    let mut ws_stream: WebSocketStream<TcpStream> = match accept_async(tcp_stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let (mut outgoing, mut incoming) = ws_stream.split();

    while let Some(message) = incoming.next().await {
        match message {
            Ok(Message::Text(text)) => {
                // Handle text message
                println!("Received text message: {}\n", text);
                handle_message(Arc::clone(&signal_state), text, &mut outgoing).await;
            },
            Ok(Message::Close(reason)) => {
                // Handle close message
                println!("Received close message. Reason: {:?}", reason);
                break;
            }
            Ok(_) => {
                // Do nothing for the other Message types
            }
            Err(err) => {
                println!("Error on addr {}, error: {:?}", &addr, &err);
                break;
            },
        }
    }

    println!("address {} has disconnected.", &addr);
}

#[tokio::main]
async fn main() -> Result<()> {
    // State of our Signal Server
    let signal_state: Arc<Mutex<SignalState>> = Arc::new(Mutex::new(SignalState::new()));

    // 1. Initialize  TCP listener to listen to requests to the Signaling Server
    let address = "127.0.0.1:8080";
    let listener = TcpListener::bind(&address).await.expect("Can't listen");

    // 2. Create a task for each new request, this will create the websocket + message handling
    while let Ok((stream, address)) = listener.accept().await {
        tokio::spawn(handle_connection(Arc::clone(&signal_state), stream, address));
    }


    // json_serialized = {"type":"caller_ice_candidate","id":"uid","ice_candidate":"ice_candidate"}

    // let offer = SignalMessage::Offer {
    //     id: String::from("uid"), 
    //     sdp: String::from("offer_info")
    // };

    // let offer_json_serialized = serde_json::to_string(&offer)?;

    // println!("json_serialized = {}", offer_json_serialized);
    // // json_serialized = {"type":"offer","id":"uid","sdp":"offer_info"}

    // let answer = SignalMessage::Answer {
    //     id: String::from("uid"), 
    //     sdp: String::from("answer_info")
    // };

    // let answer_json_serialized = serde_json::to_string(&answer)?;

    // println!("json_serialized = {}", answer_json_serialized);
    // // json_serialized = {"type":"answer","id":"uid","sdp":"answer_info"}
    

    Ok(())
}
