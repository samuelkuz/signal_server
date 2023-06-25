use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalRequest {
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
    },
    GetOffer {
        id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalResponse {
    Offer {
        sdp: String,
    },
    IceCandidate {
        ice_candidate: String,
    },
}

