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

#[derive(Clone, Debug, Deserialize)]
struct RawNetworkConfig {
    parties: Vec<RawNetworkPartyConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawNetworkPartyConfig {
    address: SocketAddr,
    certificate: String,
}

impl NetworkConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let path = path.as_ref();
        let parent_dir = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Invalid path"))?;

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

fn parse_raw_party_config(
    parent_dir: &Path,
    raw: RawNetworkPartyConfig,
) -> Result<NetworkPartyConfig, io::Error> {
    Ok(NetworkPartyConfig {
        address: raw.address,
        certificate: load_certificate(parent_dir.join(raw.certificate))?,
    })
}

pub fn load_certificate(path: impl AsRef<Path>) -> Result<Certificate, io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if let Some(Item::X509Certificate(cert)) = rustls_pemfile::read_one(&mut reader)? {
        Ok(Certificate(cert))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Invalid certificate"))
    }
}

pub fn load_private_key(path: impl AsRef<Path>) -> Result<PrivateKey, io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    if let Some(Item::PKCS8Key(key)) = rustls_pemfile::read_one(&mut reader)? {
        Ok(PrivateKey(key))
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Invalid certificate"))
    }
}
