use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use futures::{future, stream::FuturesUnordered, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{
    rustls::{
        server::AllowAnyAuthenticatedClient, Certificate, ClientConfig, PrivateKey, RootCertStore,
        ServerConfig,
    },
    TlsAcceptor, TlsConnector, TlsStream,
};

use super::{
    wrap_channel_with_bincode, BincodeStreamSink, MultipartyTransport, NetworkConfig,
    NetworkPartyConfig,
};

/// Virtual domain name for TLS certificates.
const VIRTUAL_DOMAIN_FOR_TLS: &str = "mpc";

/// Delay in milliseconds after which connection to peer is retried.
const CONNECTION_RETRY_DELAY: u64 = 1000;

/// Public certificate and its private key.
type PrivateCert = (Certificate, PrivateKey);

/// Bincode-encoded and TLS-encrypted TCP connection.
pub type NetChannel<T> = BincodeStreamSink<T, TlsStream<TcpStream>>;

/// Establish network connections for multiparty protocol.
pub async fn connect_multiparty<T>(
    config: &NetworkConfig,
    private_key: PrivateKey,
    party_id: usize,
) -> io::Result<MultipartyTransport<T, NetChannel<T>>>
where
    T: Serialize + DeserializeOwned,
{
    let this_party = &config.parties[party_id];
    let private_cert = (this_party.certificate.clone(), private_key);

    let listen_for = listen_for_parties(
        &config.parties[..party_id],
        &private_cert,
        this_party.address,
    );

    let connect_to = future::try_join_all(
        config.parties[party_id + 1..]
            .iter()
            .map(|config| connect_to_party(config, &private_cert, party_id)),
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
    parties: &[NetworkPartyConfig],
    private_cert: &PrivateCert,
    addr: SocketAddr,
) -> io::Result<Vec<TlsStream<TcpStream>>> {
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
                futures.push(accept_party(parties, private_cert, socket));
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
    private_cert: &PrivateCert,
    mut socket: TcpStream,
) -> io::Result<(TlsStream<TcpStream>, usize)> {
    let party_id = socket.read_u32().await? as usize;
    if party_id >= parties.len() {
        return Err(io::Error::new(io::ErrorKind::Other, "Invalid party ID"));
    }

    let other_cert = parties[party_id].certificate.clone();
    let tls_socket = wrap_tls_server(socket, other_cert, private_cert.clone()).await?;
    Ok((tls_socket, party_id))
}

/// Connect to party with higher ID.
async fn connect_to_party(
    other_party: &NetworkPartyConfig,
    private_cert: &PrivateCert,
    this_party_id: usize,
) -> io::Result<TlsStream<TcpStream>> {
    let mut socket = loop {
        match TcpStream::connect(other_party.address).await {
            Ok(socket) => break socket,
            _ => tokio::time::sleep(Duration::from_millis(CONNECTION_RETRY_DELAY)).await,
        }
    };

    socket.write_u32(this_party_id as u32).await?;
    socket.flush().await?;

    let other_cert = other_party.certificate.clone();
    wrap_tls_client(socket, other_cert, private_cert.clone()).await
}

/// Wrap TCP client socket with TLS layer. Authenticates both sides using specified certificates.
async fn wrap_tls_client(
    socket: TcpStream,
    other_cert: Certificate,
    private_cert: PrivateCert,
) -> io::Result<TlsStream<TcpStream>> {
    let root_cert_store = root_cert_store_from_cert(other_cert).await?;

    let tls_config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_single_cert(vec![private_cert.0], private_cert.1)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    let connector = TlsConnector::from(Arc::new(tls_config));
    let domain = VIRTUAL_DOMAIN_FOR_TLS.try_into().unwrap();
    Ok(connector.connect(domain, socket).await?.into())
}

/// Wrap incoming TCP connection with TLS layer. Authenticates both sides using specified certificates.
async fn wrap_tls_server(
    socket: TcpStream,
    other_cert: Certificate,
    private_cert: PrivateCert,
) -> io::Result<TlsStream<TcpStream>> {
    let root_cert_store = root_cert_store_from_cert(other_cert).await?;
    let client_cert_verifier = AllowAnyAuthenticatedClient::new(root_cert_store);

    let tls_config = ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(vec![private_cert.0], private_cert.1)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    Ok(acceptor.accept(socket).await?.into())
}

/// Create root certificate store from a single certificate.
async fn root_cert_store_from_cert(cert: Certificate) -> io::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    store
        .add(&cert)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    Ok(store)
}
