/*
   Copyright 2019 Supercomputing Systems AG

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.

*/
#![feature(result_option_inspect)]
#![feature(assert_matches)]
pub mod net;
pub mod rpc;
pub mod trade;

pub use ac_compose_macros::*;
pub use ac_node_api as runtime_types;
pub use ac_primitives as primitives;
pub use net::RpcClient;
pub use rpc::Api;

pub trait FromHexString {
    fn from_hex(hex: String) -> Result<Self, hex::FromHexError>
    where
        Self: Sized;
}

impl FromHexString for Vec<u8> {
    fn from_hex(hex: String) -> Result<Self, hex::FromHexError> {
        let hexstr = hex
            .trim_matches('\"')
            .to_string()
            .trim_start_matches("0x")
            .to_string();
        hex::decode(&hexstr)
    }
}

impl FromHexString for primitives::Hash {
    fn from_hex(hex: String) -> Result<Self, hex::FromHexError> {
        let vec = Vec::from_hex(hex)?;
        match vec.len() {
            32 => Ok(primitives::Hash::from_slice(&vec)),
            _ => Err(hex::FromHexError::InvalidStringLength),
        }
    }
}

pub trait DecodeHexString {
    fn decode_hex(hex: String) -> Result<Self, hex::FromHexError>
    where
        Self: Sized;
}

// TODO
impl<T: codec::Decode> DecodeHexString for T {
    fn decode_hex(hex: String) -> Result<Self, hex::FromHexError>
    where
        Self: Sized,
    {
        let raw = Vec::from_hex(hex)?;
        T::decode(&mut &raw[..]).map_err(|_| hex::FromHexError::InvalidStringLength)
    }
}

pub trait ToHexString {
    fn to_hex(&self) -> String;
}

impl<T: codec::Encode> ToHexString for T {
    fn to_hex(&self) -> String
    where
        Self: codec::Encode,
    {
        format!("0x{}", hex::encode(self.encode()))
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_hextstr_to_vec() {
        assert_eq!(Vec::from_hex("0x01020a".to_string()), Ok(vec!(1, 2, 10)));
        assert_eq!(
            Vec::from_hex("null".to_string()),
            Err(hex::FromHexError::InvalidHexCharacter { c: 'n', index: 0 })
        );
        assert_eq!(
            Vec::from_hex("0x0q".to_string()),
            Err(hex::FromHexError::InvalidHexCharacter { c: 'q', index: 1 })
        );
    }

    #[test]
    fn test_hextstr_to_hash() {
        assert_eq!(
            primitives::Hash::from_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
            ),
            Ok(primitives::Hash::from([0u8; 32]))
        );
        assert_eq!(
            primitives::Hash::from_hex("0x010000000000000000".to_string()),
            Err(hex::FromHexError::InvalidStringLength)
        );
        assert_eq!(
            primitives::Hash::from_hex("0x0q".to_string()),
            Err(hex::FromHexError::InvalidHexCharacter { c: 'q', index: 1 })
        );
    }
}
