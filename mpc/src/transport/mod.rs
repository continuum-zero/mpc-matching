mod multiparty;

pub use multiparty::*;

use std::fmt;

use tokio::io::{AsyncRead, AsyncWrite, DuplexStream};
use tokio_serde::formats::Bincode;
use tokio_util::codec::LengthDelimitedCodec;

/// Error type for channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    Send(usize),
    Recv(usize),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Send(id) => write!(f, "Error while sending message to {}", id),
            Self::Recv(id) => write!(f, "Error while receiving message from {}", id),
        }
    }
}

/// Length-framed Bincode-encoded messages channel.
pub type BincodeStreamSink<T, C> =
    tokio_serde::Framed<tokio_util::codec::Framed<C, LengthDelimitedCodec>, T, T, Bincode<T, T>>;

/// Length-framed Bincode-encoded tokio's Duplex stream.
pub type BincodeDuplex<T> = BincodeStreamSink<T, DuplexStream>;

/// Create length-framed Bincode-encoded message channel from AsyncRead/Write.
pub fn wrap_channel_with_bincode<T, C>(channel: C) -> BincodeStreamSink<T, C>
where
    C: AsyncRead + AsyncWrite,
{
    let length_delimited = tokio_util::codec::Framed::new(channel, LengthDelimitedCodec::new());
    tokio_serde::Framed::new(length_delimited, Bincode::default())
}

/// Create bidirectional Bincode-encoded channel.
pub fn bincode_duplex<T>(max_buf_size: usize) -> (BincodeDuplex<T>, BincodeDuplex<T>) {
    let (a, b) = tokio::io::duplex(max_buf_size);
    (wrap_channel_with_bincode(a), wrap_channel_with_bincode(b))
}
