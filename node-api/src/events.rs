// Copyright 2019-2021 Parity Technologies (UK) Ltd. and Supercomputing Systems AG
// and Integritee AG.
// This file is part of subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with subxt.  If not, see <http://www.gnu.org/licenses/>.

//! Module to parse chain events
//!
//! This file is very similar to subxt, except where noted.

use crate::{
    decoder::{DecodingError, RuntimeDecoder},
    error::{Error, RuntimeError},
    metadata::{EventMetadata, Metadata, MetadataError},
    Phase,
};
use ac_primitives::Hash;
use codec::{Codec, Compact, Decode, Encode, Input};
use scale_info::{TypeDef, TypeDefPrimitive};
use sp_core::Bytes;

/// Raw bytes for an Event
#[derive(Debug)]
pub struct RawEvent {
    /// The name of the pallet from whence the Event originated.
    pub pallet: String,
    /// The index of the pallet from whence the Event originated.
    pub pallet_index: u8,
    /// The name of the pallet Event variant.
    pub variant: String,
    /// The index of the pallet Event variant.
    pub variant_index: u8,
    /// The raw Event data
    pub data: Bytes,
}

/// Events decoder.
///
/// In subxt, this was generic over a `Config` type, but it's sole usage was to derive the
/// hash type. We omitted this here and use the `ac_primitives::Hash` instead.
#[derive(Debug, Clone)]
pub struct EventsDecoder {
    decoder: RuntimeDecoder,
}

impl EventsDecoder {
    /// Creates a new `EventsDecoder`.
    pub fn new(metadata: Metadata) -> Self {
        Self {
            decoder: RuntimeDecoder { metadata },
        }
    }

    /// Decode events.
    pub fn decode_events(&self, input: &mut &[u8]) -> Result<Vec<(Phase, Raw)>, Error> {
        let compact_len = <Compact<u32>>::decode(input)?;
        let len = compact_len.0 as usize;
        log::debug!("decoding {} events", len);

        let mut r = Vec::new();
        for _ in 0..len {
            // decode EventRecord
            let phase = Phase::decode(input)?;
            let pallet_index = input.read_byte()?;
            let variant_index = input.read_byte()?;
            log::debug!(
                "phase {:?}, pallet_index {}, event_variant: {}",
                phase,
                pallet_index,
                variant_index
            );
            log::debug!("remaining input: {}", hex::encode(&input));

            let event_metadata = self.decoder.metadata.event(pallet_index, variant_index)?;

            let mut event_data = Vec::<u8>::new();
            let mut event_errors = Vec::<RuntimeError>::new();
            let result =
                self.decode_raw_event(event_metadata, input, &mut event_data, &mut event_errors);
            let raw = match result {
                Ok(()) => {
                    log::debug!("raw bytes: {}", hex::encode(&event_data),);

                    let event = RawEvent {
                        pallet: event_metadata.pallet().to_string(),
                        pallet_index,
                        variant: event_metadata.event().to_string(),
                        variant_index,
                        data: event_data.into(),
                    };

                    // topics come after the event data in EventRecord
                    let topics = Vec::<Hash>::decode(input)?;
                    log::debug!("topics: {:?}", topics);

                    Raw::Event(event)
                }
                Err(err) => return Err(err),
            };

            if event_errors.is_empty() {
                r.push((phase.clone(), raw));
            }

            for err in event_errors {
                r.push((phase.clone(), Raw::Error(err)));
            }
        }
        Ok(r)
    }

    fn decode_raw_event(
        &self,
        event_metadata: &EventMetadata,
        input: &mut &[u8],
        output: &mut Vec<u8>,
        errors: &mut Vec<RuntimeError>,
    ) -> Result<(), Error> {
        log::debug!(
            "Decoding Event '{}::{}'",
            event_metadata.pallet(),
            event_metadata.event()
        );
        for arg in event_metadata.variant().fields() {
            let type_id = arg.ty().id();
            if event_metadata.pallet() == "System" && event_metadata.event() == "ExtrinsicFailed" {
                let ty = self
                    .decoder
                    .metadata
                    .resolve_type(type_id)
                    .ok_or(MetadataError::TypeNotFound(type_id))?;

                if ty.path().ident() == Some("DispatchError".to_string()) {
                    let dispatch_error = sp_runtime::DispatchError::decode(input)?;
                    log::info!("Dispatch Error {:?}", dispatch_error);
                    dispatch_error.encode_to(output);
                    let runtime_error =
                        RuntimeError::from_dispatch(&self.decoder.metadata, dispatch_error)?;
                    errors.push(runtime_error);
                    continue;
                }
            }
            self.decoder.decode_type(type_id, input, output)?
        }
        Ok(())
    }
}

/// Raw event or error event
#[derive(Debug)]
pub enum Raw {
    /// Event
    Event(RawEvent),
    /// Error
    Error(RuntimeError),
}

#[derive(Debug, thiserror::Error)]
pub enum EventsDecodingError {
    /// Unsupported primitive type
    #[error("Unsupported primitive type {0:?}")]
    UnsupportedPrimitive(TypeDefPrimitive),
    /// Invalid compact type, must be an unsigned int.
    #[error("Invalid compact primitive {0:?}")]
    InvalidCompactPrimitive(TypeDefPrimitive),
    #[error("Invalid compact composite type {0}")]
    InvalidCompactType(String),
}
