use std::fmt;

use futures::{
    stream::{SplitSink, SplitStream},
    FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt,
};
use serde::{de::DeserializeOwned, Serialize};
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

/// Wrapper for peer-to-peer connections in multi-party protocol.
pub struct MultipartyTransport<T, Channel> {
    channels: Vec<Option<(SplitSink<Channel, T>, SplitStream<Channel>)>>,
    party_id: usize,
}

impl<T, Channel> MultipartyTransport<T, Channel>
where
    Channel: Stream + Sink<T>,
{
    /// Create wrapper for given list of connections. All channels but party_id should be present.
    pub fn new(channels: impl IntoIterator<Item = Option<Channel>>, party_id: usize) -> Self {
        // We split streams into unidirectional halves. This allows us to
        // asynchronously wait on both receives and sends without bothering borrow checker.
        let channels: Vec<_> = channels.into_iter().map(|x| x.map(|x| x.split())).collect();
        for (j, channel) in channels.iter().enumerate() {
            if j != party_id && channel.is_none() {
                panic!("Channel missing for party {}", j);
            }
        }
        Self { channels, party_id }
    }
}

impl<T, Channel> MultipartyTransport<T, Channel> {
    /// Number of parties participating in multi-party protocol.
    pub fn num_parties(&self) -> usize {
        self.channels.len()
    }

    /// ID of current party.
    pub fn party_id(&self) -> usize {
        self.party_id
    }
}

impl<T, E, Channel> MultipartyTransport<T, Channel>
where
    T: Clone,
    Channel: Stream<Item = Result<T, E>> + Sink<T> + Unpin,
{
    /// Send message to party with given ID.
    pub async fn send_to(&mut self, other_id: usize, msg: T) -> Result<(), TransportError> {
        if other_id == self.party_id {
            panic!("Cannot send message on loopback");
        }
        let (sink, _) = self.channels[other_id].as_mut().unwrap();
        sink.send(msg)
            .await
            .map_err(|_| TransportError::Send(other_id))
    }

    /// Receive message from party wit given ID.
    pub async fn receive_from(&mut self, other_id: usize) -> Result<T, TransportError> {
        if other_id == self.party_id {
            panic!("Cannot receive message on loopback");
        }
        let (_, stream) = self.channels[other_id].as_mut().unwrap();
        match stream.next().await {
            Some(Ok(msg)) => Ok(msg),
            _ => Err(TransportError::Recv(other_id)),
        }
    }

    /// Send message to all parties.
    pub async fn send_to_all(&mut self, msg: T) -> Result<(), TransportError> {
        futures::future::try_join_all(
            self.channels
                .iter_mut()
                .enumerate()
                .filter(|(id, _)| *id != self.party_id)
                .map(|(id, channel)| {
                    let (sink, _) = channel.as_mut().unwrap();
                    sink.send(msg.clone())
                        .then(move |x| async move { x.map_err(|_| TransportError::Send(id)) })
                }),
        )
        .await
        .map(|_| ())
    }

    /// Receive messages from all parties.
    pub async fn receive_from_all(&mut self) -> Result<Vec<(usize, T)>, TransportError> {
        futures::future::try_join_all(
            self.channels
                .iter_mut()
                .enumerate()
                .filter(|(id, _)| *id != self.party_id)
                .map(|(id, channel)| {
                    let (_, stream) = channel.as_mut().unwrap();
                    stream.next().then(move |raw| async move {
                        match raw {
                            Some(Ok(msg)) => Ok((id, msg)),
                            _ => Err(TransportError::Recv(id)),
                        }
                    })
                }),
        )
        .await
    }

    /// Concurrently send and receive messages from all parties.
    pub async fn exchange_with_all(&mut self, msg: T) -> Result<Vec<(usize, T)>, TransportError> {
        futures::future::try_join_all(
            self.channels
                .iter_mut()
                .enumerate()
                .filter(|(id, _)| *id != self.party_id)
                .map(|(id, channel)| {
                    let (sink, stream) = channel.as_mut().unwrap();
                    let send_future = sink
                        .send(msg.clone())
                        .then(move |x| async move { x.map_err(|_| TransportError::Send(id)) });
                    let recv_future = stream.next().then(move |raw| async move {
                        match raw {
                            Some(Ok(msg)) => Ok((id, msg)),
                            _ => Err(TransportError::Recv(id)),
                        }
                    });
                    futures::future::try_join(send_future, recv_future)
                        .and_then(|(_, received_msg)| async { Ok(received_msg) })
                }),
        )
        .await
    }
}

/// Length-framed Bincode-encoded messages channel.
pub type BincodeStreamSink<T, C> =
    tokio_serde::Framed<tokio_util::codec::Framed<C, LengthDelimitedCodec>, T, T, Bincode<T, T>>;

/// Length-framed Bincode-encoded tokio's Duplex stream.
pub type BincodeDuplex<T> = BincodeStreamSink<T, DuplexStream>;

/// Create length-framed Bincode-encoded message channel from AsyncRead/Write.
pub fn wrap_bincode<T, C>(channel: C) -> BincodeStreamSink<T, C>
where
    C: AsyncRead + AsyncWrite,
{
    let length_delimited = tokio_util::codec::Framed::new(channel, LengthDelimitedCodec::new());
    tokio_serde::Framed::new(length_delimited, Bincode::default())
}

/// Create bidirectional Bincode-encoded channel.
pub fn bincode_duplex<T>(max_buf_size: usize) -> (BincodeDuplex<T>, BincodeDuplex<T>) {
    let (a, b) = tokio::io::duplex(max_buf_size);
    (wrap_bincode(a), wrap_bincode(b))
}

/// Create in-process channels for testing multiparty protocols.
pub fn mock_multiparty_channels<T>(
    num_parties: usize,
    max_buf_size: usize,
) -> Vec<MultipartyTransport<T, BincodeDuplex<T>>>
where
    T: Clone + Serialize + DeserializeOwned + Unpin,
{
    let mut matrix: Vec<Vec<_>> = (0..num_parties)
        .map(|_| (0..num_parties).map(|_| None).collect())
        .collect();

    for i in 0..num_parties {
        for j in 0..i {
            let (a, b) = bincode_duplex::<T>(max_buf_size);
            matrix[i][j] = Some(a);
            matrix[j][i] = Some(b);
        }
    }

    matrix
        .into_iter()
        .enumerate()
        .map(|(id, row)| MultipartyTransport::new(row, id))
        .collect()
}
