use std::{
    fs::File,
    io::{self, BufReader, BufWriter},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{MpcContext, MpcDealer, MpcField, MpcShare};

use super::{SpdzDealer, SpdzShare};

/// Precomputed data for SPDZ protocol.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PrecomputedSpdzData<T> {
    pub num_parties: usize,
    pub party_id: usize,
    pub auth_key: T,
    pub beaver_triples: Vec<(SpdzShare<T>, SpdzShare<T>, SpdzShare<T>)>,
    pub random_bits: Vec<SpdzShare<T>>,
    pub input_masks: Vec<Vec<SpdzShare<T>>>,
    pub input_masks_plain: Vec<T>,
}

impl<T> PrecomputedSpdzData<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    /// Load precomputed data from file.
    pub fn load_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        bincode::deserialize_from(reader).map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    /// Save precomputed data to file.
    pub fn save_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }
}

/// Dealer for SPDZ protocol that serves precomputed data.
pub struct PrecomputedSpdzDealer<T> {
    data: PrecomputedSpdzData<T>,
    is_exhausted: bool,
}

impl<T> PrecomputedSpdzDealer<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    /// Create new dealer given precomputed data.
    pub fn new(data: PrecomputedSpdzData<T>) -> Self {
        Self {
            data,
            is_exhausted: false,
        }
    }

    /// Create new dealer from file with precomputed data.
    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(PrecomputedSpdzData::load_file(path)?))
    }
}

impl<T: MpcField> PrecomputedSpdzDealer<T> {
    /// Random sharing of a secret random bit.
    fn next_bit(&mut self) -> SpdzShare<T> {
        if let Some(share) = self.data.random_bits.pop() {
            share
        } else {
            self.is_exhausted = true;
            Default::default()
        }
    }
}

impl<T: MpcField> MpcContext for PrecomputedSpdzDealer<T> {
    type Field = T;
    type Share = SpdzShare<T>;

    fn num_parties(&self) -> usize {
        self.data.num_parties
    }

    fn party_id(&self) -> usize {
        self.data.party_id
    }
}

impl<T: MpcField> MpcDealer for PrecomputedSpdzDealer<T> {
    fn share_plain(&self, x: Self::Field) -> Self::Share {
        SpdzShare::from_plain(x, self.data.auth_key, self.party_id())
    }

    fn next_beaver_triple(&mut self) -> (Self::Share, Self::Share, Self::Share) {
        if let Some(triple) = self.data.beaver_triples.pop() {
            triple
        } else {
            self.is_exhausted = true;
            (Default::default(), Default::default(), Default::default())
        }
    }

    fn next_uint(&mut self, bits: usize) -> Self::Share {
        (0..bits).fold(Self::Share::zero(), |acc, _| acc.double() + self.next_bit())
    }

    fn is_exhausted(&self) -> bool {
        self.is_exhausted
    }
}

impl<T: MpcField> SpdzDealer for PrecomputedSpdzDealer<T> {
    fn authentication_key_share(&self) -> Self::Field {
        self.data.auth_key
    }

    fn next_input_mask_own(&mut self) -> (Self::Share, Self::Field) {
        let id = self.party_id();
        if let Some(mask) = self.data.input_masks[id].pop() {
            (mask, self.data.input_masks_plain.pop().unwrap())
        } else {
            self.is_exhausted = true;
            (Default::default(), Default::default())
        }
    }

    fn next_input_mask_for(&mut self, id: usize) -> Self::Share {
        if id == self.party_id() {
            panic!("Tried to get own mask as third-party mask");
        }
        if let Some(mask) = self.data.input_masks[id].pop() {
            mask
        } else {
            self.is_exhausted = true;
            Default::default()
        }
    }
}
