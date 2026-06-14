use serde::ser::{SerializeStruct, Serializer};

use crate::Span;

pub fn serialize_span<S: Serializer>(span: &Span, serializer: S) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("Span", 2)?;
    state.serialize_field("start", &span.start)?;
    state.serialize_field("end", &span.end)?;
    state.end()
}