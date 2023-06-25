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
use crate::signal_message::{SignalRequest, SignalResponse};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct Session {
    // Caller Info
    pub offer: Option<String>,
    pub caller_ws: Option<Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>>,
    pub caller_ice_candidates: Vec<String>,
    // Peer Info
    pub answer: Option<String>,
    pub peer_ws: Option<Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>>,
    pub peer_ice_candidates: Vec<String>,
}

impl Session {
    pub fn new(offer: String, caller_ws: Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>) -> Self {
        Session {
            offer: Some(offer),
            caller_ws: Some(caller_ws),
            caller_ice_candidates: Vec::new(),
            answer: None,
            peer_ws: None,
            peer_ice_candidates: Vec::new(),
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

async fn handle_message(signal_state: Arc<Mutex<SignalState>>, message: String, ws: Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>) -> Result<()> {
    let signal_message: SignalRequest = serde_json::from_str(message.as_str())?;

    let mut current_state = signal_state.lock().await;

    match signal_message {
        SignalRequest::Offer { id, sdp } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("Updating Session Id {} from old_offer: {:?} to new_offer: {:?} this shouldn't really happen though?\n", &id, &session.offer, &sdp);
                session.offer = Some(sdp);
            } else {
                println!("Creating Session with Id {} and offer {:?}\n", &id, &sdp);
                current_state.session_map.insert(id, Session::new(sdp, Arc::clone(&ws)));
            }
        },
        SignalRequest::GetOffer { id } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                // Since I'm hardcoding this Signal server technically I can just use the peer_ws instead of using the ws from the request, but either way this should work.
                let mut request_ws = ws.lock().await;
                
                if let Some(offer) = &session.offer {
                    println!("[GetOffer] Session Id {} exists and Offer exists. Sending offer: {:?}\n", &id, &offer);

                    let offer_clone = offer.clone();
                    let offer_response = SignalResponse::Offer { sdp: offer_clone};
                    let offer_response_json = serde_json::to_string(&offer_response)?;

                    request_ws.send(Message::text(offer_response_json)).await?;
                }
            }
        },
        SignalRequest::Answer { id, sdp } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("Updating Session Id {} to include answer: {:?}\n", &id, &sdp);
                session.answer = Some(sdp);


                // session.peer_ws = Some(Arc::clone(&ws));

                // let mut caller_ws = &session.caller_ws.unwrap().lock().await;

                // for ice_candidate in session.caller_ice_candidates {
                //     let ice_clone = ice_candidate.clone();
                //     let ice_response = SignalResponse::IceCandidate { ice_candidate: ice_clone};
                //     let ice_response_json = serde_json::to_string(&ice_response)?;
                    
                //     caller_ws.send(Message::text(ice_response_json)).await;
                // }
            } else {
                println!("Session should be created before sending an Answer");
            }
        },
        SignalRequest::CallerIceCandidate { id, ice_candidate } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("Updating Session Id {} to have ice_candidate: {:?}\n", &id, &ice_candidate);
                session.caller_ice_candidates.push(ice_candidate.clone());

                if let Some(caller_ws) = &session.caller_ws {
                    let mut caller_ws = caller_ws.lock().await;

                    caller_ws.send(Message::text(ice_candidate)).await?;
                }
            } else {
                println!("A session should be created before adding ice_candidates? Look into this!\n");
            }
        },
        _ => {
            println!("Got here!\n");
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

    let outgoing = Arc::new(Mutex::new(outgoing));

    while let Some(message) = incoming.next().await {
        match message {
            Ok(Message::Text(text)) => {
                // Handle text message
                println!("Received text message: {}\n", text);
                handle_message(Arc::clone(&signal_state), text, Arc::clone(&outgoing)).await;
            },
            Ok(Message::Close(reason)) => {
                // Handle close message
                println!("Received close message. Reason: {:?}\n", reason);
                break;
            }
            Ok(_) => {
                // Do nothing for the other Message types
            }
            Err(err) => {
                println!("Error on addr {}, error: {:?}\n", &addr, &err);
                break;
            },
        }
    }

    println!("address {} has disconnected.\n", &addr);
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

    // let offer = SignalRequest::Offer {
    //     id: String::from("uid"), 
    //     sdp: String::from("offer_info")
    // };

    // let offer_json_serialized = serde_json::to_string(&offer)?;

    // println!("json_serialized = {}", offer_json_serialized);
    // // json_serialized = {"type":"offer","id":"uid","sdp":"offer_info"}

    // let answer = SignalRequest::Answer {
    //     id: String::from("uid"), 
    //     sdp: String::from("answer_info")
    // };

    // let answer_json_serialized = serde_json::to_string(&answer)?;

    // println!("json_serialized = {}", answer_json_serialized);
    // // json_serialized = {"type":"answer","id":"uid","sdp":"answer_info"}
    

    Ok(())
}
