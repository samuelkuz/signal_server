use serde::{Deserialize, Serialize};

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

