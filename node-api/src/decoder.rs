use crate::{
    error::{Error, RuntimeError},
    metadata::{EventMetadata, Metadata, MetadataError},
    Phase,
};
use ac_primitives::Hash;
use codec::{Codec, Compact, Decode, Encode, Error as CodecError, Input};
pub use frame_metadata::StorageHasher;
use scale_info::{TypeDef, TypeDefPrimitive};
use sp_core::Bytes;

#[derive(Clone, Debug)]
pub struct RuntimeDecoder {
    pub metadata: Metadata,
}

impl RuntimeDecoder {
    pub fn new(metadata: Metadata) -> Self {
        Self { metadata }
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

            let event_metadata = self.metadata.event(pallet_index, variant_index)?;

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
                    .metadata
                    .resolve_type(type_id)
                    .ok_or(MetadataError::TypeNotFound(type_id))?;

                if ty.path().ident() == Some("DispatchError".to_string()) {
                    let dispatch_error = sp_runtime::DispatchError::decode(input)?;
                    log::info!("Dispatch Error {:?}", dispatch_error);
                    dispatch_error.encode_to(output);
                    let runtime_error =
                        RuntimeError::from_dispatch(&self.metadata, dispatch_error)?;
                    errors.push(runtime_error);
                    continue;
                }
            }
            self.decode_type(type_id, input, output)?
        }
        Ok(())
    }

    pub fn decode_raw_enum<F, T>(&self, input: &mut &[u8], f: F) -> Result<T, CodecError>
    where
        F: Fn(u8, &mut &[u8]) -> Result<T, CodecError>,
    {
        f(input.read_byte()?, input)
    }

    fn extract_identifier_from_concated_key(
        hasher: StorageHasher,
        input: &mut &[u8],
    ) -> Result<(), CodecError> {
        match hasher {
            StorageHasher::Blake2_128Concat => {
                let mut x = vec![0u8; 16];
                input.read(&mut x)?;
                Ok(())
            }
            StorageHasher::Twox64Concat => {
                let mut x = vec![0u8; 8];
                input.read(&mut x)?;
                Ok(())
            }
            _ => Err("Couldn't extract identifier from a non-concated storage key".into()),
        }
    }

    pub fn extract_map_identifier<T: Decode>(
        hasher: StorageHasher,
        input: &mut &[u8],
    ) -> Result<T, Error> {
        // always start with twox128 * 2 => 16 + 16
        let mut x = vec![0u8; 32];
        input.read(&mut x)?;
        Self::extract_identifier_from_concated_key(hasher, input)?;
        T::decode(input).map_err(|e| e.into())
    }

    pub fn extract_double_map_identifier<T: Decode, K: Encode>(
        first_hasher: StorageHasher,
        second_hasher: StorageHasher,
        first_key: &K,
        input: &mut &[u8],
    ) -> Result<T, Error> {
        // always start with twox128 * 2 => 16 + 16
        let mut x = vec![0u8; 32];
        input.read(&mut x)?;
        Self::extract_identifier_from_concated_key(first_hasher, input)?;
        let mut x = vec![0u8; first_key.encoded_size()];
        input.read(&mut x)?;
        Self::extract_identifier_from_concated_key(second_hasher, input)?;
        T::decode(input).map_err(|e| e.into())
    }

    /// Decode runtime call.
    pub fn decode_call(&self, input: &mut &[u8]) -> Result<(), Error> {
        let pallet_index = input.read_byte()?;
        let variant_index = input.read_byte()?;
        let call_metadata = self.metadata.call(pallet_index, variant_index)?;
        let mut output = vec![];
        for arg in call_metadata.variant().fields() {
            let type_id = arg.ty().id();
            self.decode_type(type_id, input, &mut output)?
        }
        Ok(())
    }

    pub fn decode_type(
        &self,
        type_id: u32,
        input: &mut &[u8],
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let ty = self
            .metadata
            .resolve_type(type_id)
            .ok_or(MetadataError::TypeNotFound(type_id))?;

        fn decode_raw<T: Codec>(input: &mut &[u8], output: &mut Vec<u8>) -> Result<(), Error> {
            let decoded = T::decode(input)?;
            decoded.encode_to(output);
            Ok(())
        }

        match ty.type_def() {
            TypeDef::Composite(composite) => {
                for field in composite.fields() {
                    self.decode_type(field.ty().id(), input, output)?
                }
                Ok(())
            }
            TypeDef::Variant(variant) => {
                let variant_index = u8::decode(input)?;
                variant_index.encode_to(output);
                let variant = variant
                    .variants()
                    .get(variant_index as usize)
                    .ok_or_else(|| Error::Other(format!("Variant {} not found", variant_index)))?;
                for field in variant.fields() {
                    self.decode_type(field.ty().id(), input, output)?;
                }
                Ok(())
            }
            TypeDef::Sequence(seq) => {
                let len = <Compact<u32>>::decode(input)?;
                len.encode_to(output);
                for _ in 0..len.0 {
                    self.decode_type(seq.type_param().id(), input, output)?;
                }
                Ok(())
            }
            TypeDef::Array(arr) => {
                for _ in 0..arr.len() {
                    self.decode_type(arr.type_param().id(), input, output)?;
                }
                Ok(())
            }
            TypeDef::Tuple(tuple) => {
                for field in tuple.fields() {
                    self.decode_type(field.id(), input, output)?;
                }
                Ok(())
            }
            TypeDef::Primitive(primitive) => match primitive {
                TypeDefPrimitive::Bool => decode_raw::<bool>(input, output),
                TypeDefPrimitive::Char => {
                    Err(DecodingError::UnsupportedPrimitive(TypeDefPrimitive::Char).into())
                }
                TypeDefPrimitive::Str => decode_raw::<String>(input, output),
                TypeDefPrimitive::U8 => decode_raw::<u8>(input, output),
                TypeDefPrimitive::U16 => decode_raw::<u16>(input, output),
                TypeDefPrimitive::U32 => decode_raw::<u32>(input, output),
                TypeDefPrimitive::U64 => decode_raw::<u64>(input, output),
                TypeDefPrimitive::U128 => decode_raw::<u128>(input, output),
                TypeDefPrimitive::U256 => {
                    Err(DecodingError::UnsupportedPrimitive(TypeDefPrimitive::U256).into())
                }
                TypeDefPrimitive::I8 => decode_raw::<i8>(input, output),
                TypeDefPrimitive::I16 => decode_raw::<i16>(input, output),
                TypeDefPrimitive::I32 => decode_raw::<i32>(input, output),
                TypeDefPrimitive::I64 => decode_raw::<i64>(input, output),
                TypeDefPrimitive::I128 => decode_raw::<i128>(input, output),
                TypeDefPrimitive::I256 => {
                    Err(DecodingError::UnsupportedPrimitive(TypeDefPrimitive::I256).into())
                }
            },
            TypeDef::Compact(_compact) => {
                let inner = self
                    .metadata
                    .resolve_type(type_id)
                    .ok_or(MetadataError::TypeNotFound(type_id))?;
                let mut decode_compact_primitive = |primitive: &TypeDefPrimitive| match primitive {
                    TypeDefPrimitive::U8 => decode_raw::<Compact<u8>>(input, output),
                    TypeDefPrimitive::U16 => decode_raw::<Compact<u16>>(input, output),
                    TypeDefPrimitive::U32 => decode_raw::<Compact<u32>>(input, output),
                    TypeDefPrimitive::U64 => decode_raw::<Compact<u64>>(input, output),
                    TypeDefPrimitive::U128 => decode_raw::<Compact<u128>>(input, output),
                    prim => Err(DecodingError::InvalidCompactPrimitive(prim.clone()).into()),
                };
                match inner.type_def() {
                    TypeDef::Primitive(primitive) => decode_compact_primitive(primitive),
                    TypeDef::Composite(composite) => match composite.fields() {
                        [field] => {
                            let field_ty = self
                                .metadata
                                .resolve_type(field.ty().id())
                                .ok_or_else(|| MetadataError::TypeNotFound(field.ty().id()))?;
                            if let TypeDef::Primitive(primitive) = field_ty.type_def() {
                                decode_compact_primitive(primitive)
                            } else {
                                Err(DecodingError::InvalidCompactType(
                                    "Composite type must have a single primitive field".into(),
                                )
                                .into())
                            }
                        }
                        _ => Err(DecodingError::InvalidCompactType(
                            "Composite type must have a single field".into(),
                        )
                        .into()),
                    },
                    _ => Err(DecodingError::InvalidCompactType(
                        "Compact type must be a primitive or a composite type".into(),
                    )
                    .into()),
                }
            }
            TypeDef::BitSequence(_bitseq) => {
                // decode_raw::<bitvec::BitVec>
                unimplemented!("BitVec decoding for events not implemented yet")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    /// Unsupported primitive type
    #[error("Unsupported primitive type {0:?}")]
    UnsupportedPrimitive(TypeDefPrimitive),
    /// Invalid compact type, must be an unsigned int.
    #[error("Invalid compact primitive {0:?}")]
    InvalidCompactPrimitive(TypeDefPrimitive),
    #[error("Invalid compact composite type {0}")]
    InvalidCompactType(String),
}

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

/// Raw event or error event
#[derive(Debug)]
pub enum Raw {
    /// Event
    Event(RawEvent),
    /// Error
    Error(RuntimeError),
}
