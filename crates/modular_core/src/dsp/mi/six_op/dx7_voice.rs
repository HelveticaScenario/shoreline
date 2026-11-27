//! The voice a `$mi.sixOp` plays: one DX7 voice of a `.syx` file. The DSL
//! reads the file and sends the voice's packed bytes with the params, so the
//! patch carries the voice itself, unpacked once on the main thread.

use std::borrow::Cow;

use deserr::{DeserializeError, ErrorKind, IntoValue, Map, ValuePointerRef};
use schemars::JsonSchema;

use super::patch::{PACKED_SIZE, Patch};
use crate::patch::Patch as Graph;
use crate::types::{CableSource, Connect, LaneExpansion, PreviousIds};

#[derive(Clone)]
pub struct Dx7Voice {
    patch: Patch,
    id: u64,
}

impl Dx7Voice {
    pub(super) fn new(packed: &[u8; PACKED_SIZE]) -> Self {
        Self {
            patch: Patch::unpack(packed),
            id: fnv1a(packed),
        }
    }

    pub fn patch(&self) -> &Patch {
        &self.patch
    }

    /// Equal for voices with equal bytes, so a voice that keeps its bytes
    /// across an edit keeps playing undisturbed.
    pub fn id(&self) -> u64 {
        self.id
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn decode_hex(hex: &str) -> Option<[u8; PACKED_SIZE]> {
    let hex = hex.as_bytes();
    if hex.len() != PACKED_SIZE * 2 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = [0u8; PACKED_SIZE];
    for (byte, &[high, low]) in out.iter_mut().zip(hex.as_chunks::<2>().0) {
        *byte = digit(high)? << 4 | digit(low)?;
    }
    Some(out)
}

/// `bytes` as the lowercase hex a voice's `data` holds.
pub(super) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Connect for Dx7Voice {
    fn apply_default_connections(&mut self) {}
    fn connect(&mut self, _patch: &Graph) {}
    fn collect_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn collect_lane_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn inject_index_ptr(&mut self, _ptr: *const std::cell::Cell<usize>) {}
    fn expand_lanes(&mut self, _expansion: &LaneExpansion) {}
    fn mark_glides(&mut self, _old: Option<&Self>, _generation: u64, _ids: &dyn PreviousIds) {}
}

impl<E: DeserializeError> deserr::Deserr<E> for Dx7Voice {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> Result<Self, E> {
        let unexpected = |msg: &str| {
            deserr::take_cf_content(E::error::<V>(
                None,
                ErrorKind::Unexpected {
                    msg: msg.to_string(),
                },
                location,
            ))
        };
        let deserr::Value::Map(mut map) = value else {
            return Err(unexpected(
                "a DX7 voice is a .syx path, optionally followed by #name",
            ));
        };
        let data = map
            .remove("data")
            .and_then(|v| match v.into_value() {
                deserr::Value::String(s) => Some(s),
                _ => None,
            })
            .ok_or_else(|| {
                deserr::take_cf_content(E::error::<V>(
                    None,
                    ErrorKind::MissingField { field: "data" },
                    location,
                ))
            })?;
        let packed =
            decode_hex(&data).ok_or_else(|| unexpected("a DX7 voice's data is 256 hex digits"))?;
        Ok(Self::new(&packed))
    }
}

impl JsonSchema for Dx7Voice {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Dx7Voice")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "type": { "type": "string", "const": "dx7_voice" },
                "path": { "type": "string" },
                "name": { "type": "string" },
                "data": { "type": "string", "pattern": "^[0-9a-f]{256}$" }
            },
            "required": ["type", "path", "name", "data"]
        }))
        .expect("Dx7Voice schema is valid JSON Schema")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let bytes: [u8; PACKED_SIZE] = std::array::from_fn(|i| (i * 7) as u8 & 0x7f);
        assert_eq!(decode_hex(&encode_hex(&bytes)), Some(bytes));
        assert_eq!(decode_hex("00"), None);
        assert_eq!(decode_hex(&"zz".repeat(PACKED_SIZE)), None);
    }
}
