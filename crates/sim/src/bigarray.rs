//! Serde support for fixed arrays longer than 32 elements, which serde's own impls stop at. The array is written as
//! a sequence and read back as one of exactly the same length. Use as `#[serde(with = "crate::bigarray")]`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer, T: Serialize, const N: usize>(a: &[T; N], s: S) -> Result<S::Ok, S::Error> {
    a.as_slice().serialize(s)
}

pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>, const N: usize>(d: D) -> Result<[T; N], D::Error> {
    let v = Vec::<T>::deserialize(d)?;
    let n = v.len();
    v.try_into()
        .map_err(|_| serde::de::Error::invalid_length(n, &"an array of the fixed length"))
}
