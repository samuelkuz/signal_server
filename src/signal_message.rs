use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RtcIceCandidate {
    address: Option<String>,
    candidate: Option<String>,
    component: Option<String>,
    foundation: Option<String>,
    port: Option<u16>,
    priority: Option<u64>,
    protocol: Option<String>,
    relatedAddress: Option<String>,
    relatedPort: Option<u16>,
    sdpMid: Option<String>,
    sdpMLineIndex: Option<u16>,
    tcpType: Option<String>,
    r#type: Option<String>,
    usernameFragment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalMessage {
    Offer {
        id: String,
        sdp: String,
    },
    Answer {
        id: String,
        sdp: String,
    },
    CallerIceCandidate {
        id: String,
        ice_candidate: String,
    }
}

