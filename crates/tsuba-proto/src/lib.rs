//! Razer protocol 2.5: 90-byte feature-report framing, the command table and
//! payload decoders. Transport-agnostic; see `tsuba-hid` for hidraw.

pub mod chroma;
pub mod commands;
pub mod decode;
pub mod events;
pub mod macros;
pub mod profiles;
mod report;

pub use report::{Command, ProtoError, Report, Status, ARGS_LEN, REPORT_LEN};
