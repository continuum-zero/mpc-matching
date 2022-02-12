use std::{
    fs::File,
    io::{self, BufReader},
    net::SocketAddr,
    path::Path,
};

use rustls_pemfile::Item;
use serde::Deserialize;
use tokio_rustls::rustls::{Certificate, PrivateKey};

/// Configuration of networked multi-party transport.
#[derive(Clone, Debug)]
pub struct NetworkConfig {
    pub parties: Vec<NetworkPartyConfig>,
}

/// Details about party in networked multiparty protocol.
#[derive(Clone, Debug)]
pub struct NetworkPartyConfig {
    pub address: SocketAddr,
    pub certificate: Certificate,
}

/// Raw parsed JSON configuration file.
#[derive(Clone, Debug, Deserialize)]
struct RawNetworkConfig {
    parties: Vec<RawNetworkPartyConfig>,
}

/// Raw parsed JSON party configuration file.
#[derive(Clone, Debug, Deserialize)]
pub struct RawNetworkPartyConfig {
    address: SocketAddr,
    certificate: String,
}

impl NetworkConfig {
    /// Load configuration from JSON file.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let parent_dir = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid path"))?;

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let raw: RawNetworkConfig = serde_json::from_reader(reader)?;

        Ok(NetworkConfig {
            parties: raw
                .parties
                .into_iter()
                .map(|x| parse_raw_party_config(parent_dir, x))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Parse raw party configuration.
fn parse_raw_party_config(
    parent_dir: &Path,
    raw: RawNetworkPartyConfig,
) -> io::Result<NetworkPartyConfig> {
    Ok(NetworkPartyConfig {
        address: raw.address,
        certificate: load_certificate(parent_dir.join(raw.certificate))?,
    })
}

/// Load X.509 certificate from file.
pub fn load_certificate(path: impl AsRef<Path>) -> io::Result<Certificate> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if let Some(Item::X509Certificate(cert)) = rustls_pemfile::read_one(&mut reader)? {
        Ok(Certificate(cert))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Invalid certificate"))
    }
}

/// Load PKCS#8 private key from file.
pub fn load_private_key(path: impl AsRef<Path>) -> io::Result<PrivateKey> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if let Some(Item::PKCS8Key(key)) = rustls_pemfile::read_one(&mut reader)? {
        Ok(PrivateKey(key))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Invalid certificate"))
    }
}
