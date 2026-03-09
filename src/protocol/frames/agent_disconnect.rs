use crate::protocol::{
    SpopFrame,
    frame::{FrameFlags, FramePayload, FrameType, Metadata},
    types::TypedData,
};
use std::collections::HashMap;

/// Frame AGENT-DISCONNECT
///
/// <https://github.com/haproxy/haproxy/blob/master/doc/SPOE.txt#L979>
///
/// ```text
/// 3.2.9. Frame: AGENT-DISCONNECT
/// -------------------------------
///
/// If an error occurs, at anytime, from the agent size, a AGENT-DISCONNECT frame
/// is sent, with information describing the error. such frame is also sent in reply
/// to a HAPROXY-DISCONNECT. The agent must close the socket just after sending
/// this frame.
///
/// The payload of this frame is a KV-LIST. STREAM-ID and FRAME-ID are must be set
/// 0.
///
/// Following items are mandatory in the KV-LIST:
///
///   * "status-code"    <UINT32>
///
///     This is the code corresponding to the error.
///
///   * "message"    <STRING>
///
///     This is a textual message describing the error.
///
/// For more information about known errors, see section "Errors & timeouts"
/// ```
#[derive(Debug)]
pub struct AgentDisconnect {
    pub status_code: u32,
    pub message: String,
}

impl AgentDisconnect {
    pub fn to_kv_list(&self) -> HashMap<String, TypedData> {
        let mut map = HashMap::new();

        map.insert(
            "status-code".to_string(),
            TypedData::UInt32(self.status_code),
        );

        map.insert(
            "message".to_string(),
            TypedData::String(self.message.clone()),
        );

        map
    }
}

#[derive(Debug)]
pub struct AgentDisconnectFrame {
    pub metadata: Metadata,
    pub payload: AgentDisconnect,
}

impl AgentDisconnectFrame {
    pub fn new(status_code: u32, message: String) -> Self {
        Self {
            metadata: Metadata {
                flags: FrameFlags::new(true, false), // FIN flag set, ABORT flag not set
                stream_id: 0,
                frame_id: 0,
            },
            payload: AgentDisconnect {
                status_code,
                message,
            },
        }
    }
}

impl SpopFrame for AgentDisconnectFrame {
    fn frame_type(&self) -> &FrameType {
        &FrameType::AgentDisconnect
    }

    fn metadata(&self) -> Metadata {
        self.metadata.clone()
    }

    fn payload(&self) -> FramePayload {
        let map = self.payload.to_kv_list();
        FramePayload::KVList(map)
    }
}

impl TryFrom<FramePayload> for AgentDisconnect {
    type Error = String;

    fn try_from(payload: FramePayload) -> Result<Self, Self::Error> {
        // Ensure that the payload is a KVList
        if let FramePayload::KVList(kv_list) = payload {
            let status_code = kv_list
                .get("status-code")
                .and_then(|v| match v {
                    TypedData::UInt32(val) => Some(*val),
                    _ => None,
                })
                .ok_or_else(|| "Missing or invalid status_code".to_string())?;

            let message = kv_list
                .get("message")
                .and_then(|v| match v {
                    TypedData::String(val) => Some(val.clone()),
                    _ => None,
                })
                .ok_or_else(|| "Missing message".to_string())?;

            Ok(Self {
                status_code,
                message,
            })
        } else {
            Err("Invalid FramePayload type, expected KVList.".to_string())
        }
    }
}
