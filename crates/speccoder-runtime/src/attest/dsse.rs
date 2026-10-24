//! DSSE envelopes and the in-toto Bundle that holds them.
//!
//! A signature covers `PAE(payloadType, payload)`, never the JSON around it
//! (DSSE v1); a Bundle is JSON Lines, one envelope per line, in no
//! particular order.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The payload type of every in-toto statement.
pub const PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// The bytes a DSSE signature covers:
/// `"DSSEv1" SP LEN(type) SP type SP LEN(body) SP body`.
#[must_use]
pub fn pae(payload_type: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "DSSEv1 {} {payload_type} {} ",
        payload_type.len(),
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// One signature on an envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// The key's fingerprint (`SHA256:…` for SSH, the OpenPGP fingerprint).
    pub keyid: String,
    /// Base64 of the signature as the signer wrote it (armored text).
    pub sig: String,
}

/// A DSSE envelope: one statement and its signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// [`PAYLOAD_TYPE`].
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    /// Base64 of the statement's bytes.
    pub payload: String,
    /// Empty when unsigned.
    pub signatures: Vec<Signature>,
    /// The Sigstore signer's verification material (certificate, log
    /// entry); other consumers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigstore: Option<Value>,
}

impl Envelope {
    /// An unsigned envelope around `statement`'s canonical bytes.
    #[must_use]
    pub fn unsigned(statement: &[u8]) -> Self {
        Self {
            payload_type: PAYLOAD_TYPE.to_owned(),
            payload: STANDARD.encode(statement),
            signatures: Vec::new(),
            sigstore: None,
        }
    }

    /// The statement's bytes; `None` when the payload is not base64.
    #[must_use]
    pub fn body(&self) -> Option<Vec<u8>> {
        STANDARD.decode(&self.payload).ok()
    }

    /// The bytes a signature on this envelope covers.
    #[must_use]
    pub fn signed_bytes(&self) -> Option<Vec<u8>> {
        self.body().map(|b| pae(&self.payload_type, &b))
    }

    /// The statement, parsed.
    #[must_use]
    pub fn statement(&self) -> Option<Value> {
        serde_json::from_slice(&self.body()?).ok()
    }
}

/// Encodes a bundle: one envelope per line.
#[must_use]
pub fn encode(lines: &[Envelope]) -> String {
    lines
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .map(|l| l + "\n")
        .collect()
}

/// Decodes a bundle. Lines that are not envelopes are counted, not kept:
/// a bundle may hold other tools' lines, and a line nobody can read is a
/// finding for `attest verify`, not a reason to drop the rest.
#[must_use]
pub fn decode(text: &str) -> (Vec<Envelope>, usize) {
    let mut lines = Vec::new();
    let mut unreadable = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match serde_json::from_str::<Envelope>(line) {
            Ok(e) => lines.push(e),
            Err(_) => unreadable += 1,
        }
    }
    (lines, unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pae_matches_the_dsse_specification() {
        // The DSSE v1 protocol's own example.
        assert_eq!(
            pae("http://example.com/HelloWorld", b"hello world"),
            b"DSSEv1 29 http://example.com/HelloWorld 11 hello world"
        );
        assert_eq!(pae("", b""), b"DSSEv1 0  0 ");
    }

    #[test]
    fn a_bundle_round_trips_and_counts_what_it_cannot_read() {
        let e = Envelope::unsigned(br#"{"a":1}"#);
        let text = format!("{}not json\n\n", encode(&[e.clone(), e.clone()]));
        let (lines, unreadable) = decode(&text);
        assert_eq!((lines.len(), unreadable), (2, 1));
        assert_eq!(lines[0].statement(), Some(serde_json::json!({ "a": 1 })));
        assert_eq!(
            lines[0].signed_bytes().unwrap(),
            b"DSSEv1 28 application/vnd.in-toto+json 7 {\"a\":1}"
        );
    }
}
