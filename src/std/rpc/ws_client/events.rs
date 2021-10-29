// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of substrate-subxt.
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
// along with substrate-subxt.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Compact, Decode, Encode, Error as CodecError, Input, Output};
use sp_runtime::DispatchError;
use support::weights::DispatchInfo;
use system::Phase;

use crate::std::node_metadata::{EventArg, Metadata, MetadataError};
use crate::std::AccountId;
use crate::Hash;

/// Event for the System module.
#[derive(Clone, Debug, Decode)]
pub enum SystemEvent {
    /// An extrinsic completed successfully.
    ExtrinsicSuccess(DispatchInfo),
    /// An extrinsic failed.
    ExtrinsicFailed(DispatchError, DispatchInfo),
    CodeUpdated,
    NewAccount(AccountId),
    KilledAccount(AccountId),
    Remarked(AccountId, Hash),
}

/// Top level Event that can be produced by a substrate runtime
#[derive(Debug)]
pub enum RuntimeEvent {
    System(SystemEvent),
    Raw(RawEvent),
}

/// Raw bytes for an Event
#[derive(Debug)]
pub struct RawEvent {
    /// The name of the module from whence the Event originated
    pub module: String,
    /// The name of the Event
    pub variant: String,
    /// The raw Event data
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("Scale codec error: {0:?}")]
    CodecError(#[from] CodecError),
    #[error("Metadata error: {0:?}")]
    Metadata(#[from] MetadataError),
    #[error("Type Sizes Unavailable: {0:?}")]
    TypeSizeUnavailable(String),
    #[error("Module error: {0:?}")]
    ModuleError(String),
}

#[derive(Clone)]
pub struct EventsDecoder {
    metadata: Metadata,
    // marker: PhantomData<fn() -> T>,
}

impl From<Metadata> for EventsDecoder {
    fn from(metadata: Metadata) -> Self {
        Self {
            metadata,
            // marker: PhantomData,
        }
    }
}

impl EventsDecoder {
    fn decode_raw_bytes<I: Input, W: Output>(
        &self,
        args: &[EventArg],
        input: &mut I,
        output: &mut W,
    ) -> Result<(), EventsError> {
        for arg in args {
            match arg {
                EventArg::Vec(arg) => {
                    let len = <Compact<u32>>::decode(input)?;
                    len.encode_to(output);
                    for _ in 0..len.0 {
                        self.decode_raw_bytes(&[*arg.clone()], input, output)?
                    }
                }
                EventArg::Tuple(args) => self.decode_raw_bytes(args, input, output)?,
                EventArg::Primitive(name, size) => {
                    if name.contains("PhantomData") || *size == 0usize {
                        return Ok(());
                    }
                    let mut buf = vec![0; *size];
                    log::debug!("type_name={:?}, size={:?}", name, size);
                    input.read(&mut buf)?;
                    output.write(&buf);
                }
                EventArg::Enum(args) => {
                    let var = input.read_byte()?;
                    self.decode_raw_bytes(&[args[var as usize].clone()], input, output)?;
                }
                EventArg::Ignore(arg) | EventArg::Alias(arg) => {
                    return Err(EventsError::TypeSizeUnavailable(arg.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn decode_events(
        &self,
        input: &mut &[u8],
    ) -> Result<Vec<(Phase, RuntimeEvent)>, EventsError> {
        log::debug!("Decoding compact len: {:?}", input);
        let compact_len = <Compact<u32>>::decode(input)?;
        let len = compact_len.0 as usize;
        let mut r = Vec::new();
        for _ in 0..len {
            log::debug!("Decoding phase: {:?}", input);
            let phase = Phase::decode(input)?;
            let module_variant = input.read_byte()?;
            let module = self.metadata.module_with_events(module_variant)?;
            let event = if module.name() == "System" {
                log::debug!("Decoding system event, intput: {:?}", input);
                let system_event = SystemEvent::decode(input)?;
                log::debug!("Decoding successful, system_event: {:?}", system_event);
                RuntimeEvent::System(system_event)
                // TODO
                // match system_event {
                //     SystemEvent::ExtrinsicSuccess(_info) => RuntimeEvent::System(system_event),
                //     SystemEvent::ExtrinsicFailed(dispatch_error, _info) => match dispatch_error {
                //         DispatchError::Module { index, error, .. } => {
                //             let module = self.metadata.module_with_errors(index)?;
                //             log::debug!("Found module events {:?}", module.name());
                //             let error_metadata = module.error(error)?;
                //             log::debug!("received error '{}::{}'", module.name(), error_metadata);
                //             return Err(EventsError::ModuleError(error_metadata.to_owned()));
                //         }
                //         _ => {
                //             log::debug!("Ignoring unsupported ExtrinsicFailed event");
                //             RuntimeEvent::System(system_event)
                //         }
                //     },
                //     _ => RuntimeEvent::System(system_event),
                // }
            } else {
                let event_variant = input.read_byte()?;
                let event_metadata = module.event(event_variant)?;
                log::debug!(
                    "decoding event '{}::{}'",
                    module.name(),
                    event_metadata.name
                );
                log::debug!("{:?}", event_metadata);
                let mut event_data = Vec::<u8>::new();
                self.decode_raw_bytes(&event_metadata.arguments(), input, &mut event_data)?;
                log::debug!(
                    "received event '{}::{}', raw bytes: {}",
                    module.name(),
                    event_metadata.name,
                    hex::encode(&event_data),
                );
                RuntimeEvent::Raw(RawEvent {
                    module: module.name().to_string(),
                    variant: event_metadata.name.clone(),
                    data: event_data,
                })
            };
            // topics come after the event data in EventRecord
            log::debug!("Phase {:?}, Event: {:?}", phase, event);
            let topics = Vec::<Hash>::decode(input)?;
            log::debug!("Topics: {:?}", topics);
            r.push((phase, event));
        }
        Ok(r)
    }
}
