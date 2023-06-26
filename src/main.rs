mod signal_message;

use failure::{Error};
use std::collections::{HashMap};
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
                println!("[Offer] Updating Session Id {} from old_offer: {:?} to new_offer: {:?} this shouldn't really happen though?\n", &id, &session.offer, &sdp);
                session.offer = Some(sdp);
            } else {
                println!("[Offer] Creating Session with Id {} and offer {:?}\n", &id, &sdp);
                current_state.session_map.insert(id, Session::new(sdp, Arc::clone(&ws)));
            }
        },
        SignalRequest::CallerIceCandidate { id, ice_candidate } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("[CallerIceCandidate] Updating Session Id {} to have caller ice_candidate: {:?}\n", &id, &ice_candidate);
                session.caller_ice_candidates.push(ice_candidate.clone());

                if let Some(peer_ws) = &session.peer_ws {
                    println!("[CallerIceCandidate] Sending peer ice_candidate\n");
                    let mut peer_ws = peer_ws.lock().await;

                    let ice_response = SignalResponse::IceCandidate { ice_candidate: ice_candidate};
                    let ice_response_json = serde_json::to_string(&ice_response)?;

                    peer_ws.send(Message::text(ice_response_json)).await?;
                }
            } else {
                println!("A session should be created before adding ice_candidates? Look into this!\n");
            }
        },
        SignalRequest::GetOffer { id } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                // This request should always come from the peer for our use-case, though we don't have proper error handling if that is not the case.
                session.peer_ws = Some(ws);

                if let Some(offer) = &session.offer {
                    println!("[GetOffer] Session Id {} exists and Offer exists. Sending offer to peer: {:?}\n", &id, &offer);

                    let mut peer_ws = session.peer_ws.as_ref().unwrap().lock().await;

                    let offer_clone = offer.clone();
                    let offer_response = SignalResponse::Offer { sdp: offer_clone};
                    let offer_response_json = serde_json::to_string(&offer_response)?;

                    peer_ws.send(Message::text(offer_response_json)).await?;
                }
            }
        },
        SignalRequest::Answer { id, sdp } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                let answer_clone = sdp.clone();

                println!("[Answer] Updating Session Id {} to include answer: {:?}\n", &id, &sdp);
                session.answer = Some(sdp);

                if let Some(caller_ws) = &session.caller_ws {
                    println!("[Answer] Sending caller answer: {}\n", &answer_clone);
                    let mut caller_ws = caller_ws.lock().await;

                    let answer_response = SignalResponse::Answer { sdp: answer_clone};
                    let answer_response_json = serde_json::to_string(&answer_response)?;

                    caller_ws.send(Message::text(answer_response_json)).await;
                }

                if let Some(peer_ws) = &session.peer_ws {
                    println!("[Answer] Sending peer caller's ice candidates\n");
                    let mut peer_ws = peer_ws.lock().await;

                    for ice_candidate in &session.caller_ice_candidates {
                        let ice_clone = ice_candidate.clone();
                        let ice_response = SignalResponse::IceCandidate { ice_candidate: ice_clone};
                        let ice_response_json = serde_json::to_string(&ice_response)?;

                        peer_ws.send(Message::text(ice_response_json)).await;
                    }
                }
            } else {
                println!("Session should be created before sending an Answer");
            }
        },
        SignalRequest::PeerIceCandidate { id, ice_candidate } => {
            if let Some(session) = current_state.session_map.get_mut(&id) {
                println!("[PeerIceCandidate] Updating Session Id {} to have peer ice_candidate: {:?}\n", &id, &ice_candidate);
                session.peer_ice_candidates.push(ice_candidate.clone());

                if let Some(caller_ws) = &session.caller_ws {
                    println!("[PeerIceCandidate] Sending caller new ice_candidate: {}", &ice_candidate);
                    let mut caller_ws = caller_ws.lock().await;

                    let ice_response = SignalResponse::IceCandidate { ice_candidate: ice_candidate};
                    let ice_response_json = serde_json::to_string(&ice_response)?;

                    caller_ws.send(Message::text(ice_response_json)).await?;
                }
            } else {
                println!("A session should be created before adding ice_candidates? Look into this!\n");
            }
        }
    }

    Ok(())
}

async fn handle_connection(signal_state: Arc<Mutex<SignalState>>, tcp_stream: TcpStream, addr: SocketAddr) {
    let ws_stream: WebSocketStream<TcpStream> = match accept_async(tcp_stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let (outgoing, mut incoming) = ws_stream.split();

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
    // Assigned to 0.0.0.0 while testing on local devices
    let address = "0.0.0.0:8080";
    let listener = TcpListener::bind(&address).await.expect("Can't listen");

    // 2. Create a task for each new request, this will create the websocket + message handling
    while let Ok((stream, address)) = listener.accept().await {
        tokio::spawn(handle_connection(Arc::clone(&signal_state), stream, address));
    }

    Ok(())
}
