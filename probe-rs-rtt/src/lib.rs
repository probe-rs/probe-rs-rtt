use thiserror::Error;

mod channel;
pub use channel::*;

mod rtt;
pub use rtt::*;

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "RTT control block not found in memory. Make sure RTT is initialized on the target."
    )]
    ControlBlockNotFound,

    #[error("Multiple control blocks found in memory.")]
    MultipleControlBlocksFound(Vec<u32>),

    #[error("Invalid channel number.")]
    NoSuchChannel,

    #[error("Control block corrupted: {0}")]
    ControlBlockCorrupted(String),

    #[error("The target flags contain an invalid channel mode.")]
    InvalidChannelMode,

    #[error("Error communicating with probe: {0}")]
    Probe(#[from] probe_rs::Error),
}
