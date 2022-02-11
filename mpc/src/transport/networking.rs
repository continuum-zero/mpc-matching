use std::{io, net::SocketAddr, time::Duration};

use futures::{future, stream::FuturesUnordered, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use super::{wrap_channel_with_bincode, BincodeStreamSink, MultipartyTransport};

/// Delay in milliseconds after which connection to peer is retried.
const CONNECTION_RETRY_DELAY: u64 = 1000;

/// Bincode-encoded network channel.
pub type NetChannel<T> = BincodeStreamSink<T, TcpStream>;

/// Details about party in networked multiparty protocol.
#[derive(Clone, Debug)]
pub struct NetworkPartyConfig {
    pub addr: SocketAddr,
}

/// Establish network connections for multiparty protocol.
pub async fn connect_multiparty<T>(
    parties: &[NetworkPartyConfig],
    party_id: usize,
) -> Result<MultipartyTransport<T, NetChannel<T>>, io::Error>
where
    T: Serialize + DeserializeOwned,
{
    let this_party = &parties[party_id];
    let listen_for = listen_for_parties(this_party.addr, &parties[..party_id]);

    let connect_to = future::try_join_all(
        parties[party_id + 1..]
            .iter()
            .map(|config| connect_to_party(config, party_id)),
    );

    let (listen_for, connect_to) = futures::try_join!(listen_for, connect_to)?;

    let channels = listen_for
        .into_iter()
        .map(Some)
        .chain(std::iter::once(None))
        .chain(connect_to.into_iter().map(Some))
        .map(|x| x.map(wrap_channel_with_bincode));

    Ok(MultipartyTransport::new(channels, party_id))
}

/// Listen for incoming connections from parties with lower IDs.
async fn listen_for_parties(
    addr: SocketAddr,
    parties: &[NetworkPartyConfig],
) -> Result<Vec<TcpStream>, io::Error> {
    if parties.is_empty() {
        return Ok(Vec::new());
    }

    let listener = TcpListener::bind(addr).await?;
    let mut futures = FuturesUnordered::new();
    let mut connected_parties: Vec<_> = parties.iter().map(|_| None).collect();

    loop {
        tokio::select! {
            tmp = listener.accept() => {
                let (socket, _) = tmp?;
                futures.push(accept_party(parties, socket));
            },
            tmp = futures.next(), if !futures.is_empty() => {
                if let Some(Ok((socket, id))) = tmp {
                    if connected_parties[id].is_none() {
                        connected_parties[id] = Some(socket);
                        if connected_parties.iter().all(|x| x.is_some()) {
                            break;
                        }
                    }
                }
            },
        }
    }

    Ok(connected_parties
        .into_iter()
        .map(|party| party.unwrap())
        .collect())
}

/// Process incoming connection from party.
async fn accept_party(
    parties: &[NetworkPartyConfig],
    mut socket: TcpStream,
) -> Result<(TcpStream, usize), io::Error> {
    if socket.read_u32().await? != 0xDEADBEEF {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid magic"));
    }

    let party_id = socket.read_u32().await? as usize;
    if party_id >= parties.len() {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid party ID"));
    }

    socket.write_u32(0xDEADBEEF).await?;
    socket.flush().await?;

    Ok((socket, party_id))
}

/// Connect to party with higher ID.
async fn connect_to_party(
    other_party: &NetworkPartyConfig,
    this_party_id: usize,
) -> Result<TcpStream, io::Error> {
    let mut socket = loop {
        match TcpStream::connect(other_party.addr).await {
            Ok(socket) => break socket,
            _ => tokio::time::sleep(Duration::from_millis(CONNECTION_RETRY_DELAY)).await,
        }
    };

    socket.write_u32(0xDEADBEEF).await?;
    socket.write_u32(this_party_id as u32).await?;
    socket.flush().await?;

    if socket.read_u32().await? != 0xDEADBEEF {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid magic"));
    }

    Ok(socket)
}
