use std::mem;

use async_trait::async_trait;
use digest::Digest;
use futures::{Sink, Stream};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{
    transport::{ChannelError, MultipartyTransport},
    MpcContext, MpcEngine,
};

use super::{SpdzDealer, SpdzDigest, SpdzDigestOutput, SpdzRng, SpdzShare};

const BATCH_CHECK_FREQUENCY: usize = 50000;

/// SPDZ protocol message.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum SpdzMessage<T> {
    StateHashCheck(SpdzDigestOutput),
    Commitment(SpdzDigestOutput),
    RevealCommitment(T, SpdzDigestOutput),
    MaskedInputs(Vec<T>),
    PartialOpenShares(Vec<T>),
    PartialOpenSum(Vec<T>),
}

/// SPDZ error.
#[derive(Copy, Clone, Debug)]
pub enum SpdzError {
    Send,
    Recv,
    Protocol(&'static str),
}

struct PartiallyOpenedValue<T> {
    plain_value: T,
    mac_share: T,
}

impl From<ChannelError> for SpdzError {
    fn from(err: ChannelError) -> Self {
        match err {
            ChannelError::Send => SpdzError::Send,
            ChannelError::Recv => SpdzError::Recv,
        }
    }
}

/// SPDZ protocol implementation.
pub struct SpdzEngine<T, Dealer, Channel> {
    dealer: Dealer,
    transport: MultipartyTransport<SpdzMessage<T>, Channel>,
    opened_values: Vec<PartiallyOpenedValue<T>>,
    state_digest: SpdzDigest,
    rng: SpdzRng,
}

impl<T, Dealer, Channel> SpdzEngine<T, Dealer, Channel> {
    /// Create SPDZ protocol engine.
    pub fn new(dealer: Dealer, transport: MultipartyTransport<SpdzMessage<T>, Channel>) -> Self {
        Self {
            dealer,
            transport,
            opened_values: Vec::new(),
            state_digest: SpdzDigest::new(),
            rng: SpdzRng::from_entropy(),
        }
    }
}

impl<T, Dealer, Channel> MpcContext for SpdzEngine<T, Dealer, Channel>
where
    T: ff::PrimeField,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
{
    type Field = T;
    type Share = SpdzShare<T>;

    fn num_parties(&self) -> usize {
        self.transport.num_parties()
    }

    fn party_id(&self) -> usize {
        self.transport.party_id()
    }
}

#[async_trait(?Send)]
impl<T, E, Dealer, Channel> MpcEngine for SpdzEngine<T, Dealer, Channel>
where
    T: ff::PrimeField,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = Result<SpdzMessage<T>, E>> + Sink<SpdzMessage<T>> + Unpin,
{
    type Dealer = Dealer;
    type Error = SpdzError;

    fn dealer(&mut self) -> &mut Self::Dealer {
        &mut self.dealer
    }

    async fn process_inputs(
        &mut self,
        inputs: Vec<Self::Field>,
    ) -> Result<Vec<Vec<Self::Share>>, SpdzError> {
        let (own_shares, own_deltas): (Vec<_>, Vec<_>) = inputs
            .into_iter()
            .map(|x| {
                let (share, plain) = self.dealer.next_input_mask_own();
                let delta = x - plain;
                (share + self.dealer.share_plain(delta), delta)
            })
            .unzip();

        let received_messages = self
            .transport
            .exchange_with_all(SpdzMessage::MaskedInputs(own_deltas.clone()))
            .await?;

        let mut all_shares = vec![Vec::new(); self.num_parties()];
        let mut all_deltas = vec![Vec::new(); self.num_parties()];
        all_shares[self.party_id()] = own_shares;
        all_deltas[self.party_id()] = own_deltas;

        for (other_id, msg) in received_messages {
            if let SpdzMessage::MaskedInputs(deltas) = msg {
                let (other_shares, other_deltas) = deltas
                    .into_iter()
                    .map(|delta| {
                        let share = self.dealer.next_input_mask_for(other_id);
                        (share + self.dealer.share_plain(delta), delta)
                    })
                    .unzip();
                all_shares[other_id] = other_shares;
                all_deltas[other_id] = other_deltas;
            } else {
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        for deltas in all_deltas {
            self.state_digest.update(deltas.len().to_le_bytes());
            for x in deltas {
                self.state_digest.update(x.to_repr());
            }
        }
        self.check_state_hash().await?;
        Ok(all_shares)
    }

    async fn process_openings_unchecked(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, SpdzError> {
        let mut values: Vec<_> = requests.iter().map(|x| x.value).collect();
        let values_count = values.len();

        if self.party_id() == 0 {
            for (_, msg) in self.transport.receive_from_all().await? {
                if let SpdzMessage::PartialOpenShares(parts) = msg {
                    if parts.len() != values_count {
                        return Err(SpdzError::Protocol("Received incorrect number of shares"));
                    }
                    for (i, part) in parts.into_iter().enumerate() {
                        values[i] += part;
                    }
                } else {
                    return Err(SpdzError::Protocol("Unexpected message"));
                }
            }
            self.transport
                .send_to_all(SpdzMessage::PartialOpenSum(values.clone()))
                .await?;
        } else {
            self.transport
                .send_to(0, SpdzMessage::PartialOpenShares(values))
                .await?;
            if let SpdzMessage::PartialOpenSum(sum) = self.transport.receive_from(0).await? {
                if sum.len() != values_count {
                    return Err(SpdzError::Protocol("Received incorrect number of shares"));
                }
                values = sum;
            } else {
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        // Save opened values for batch MAC and broadcast checking.
        self.opened_values
            .extend(values.iter().zip(requests.iter()).map(|(value, share)| {
                PartiallyOpenedValue {
                    plain_value: *value,
                    mac_share: share.mac,
                }
            }));

        if self.opened_values.len() >= BATCH_CHECK_FREQUENCY {
            self.check_integrity().await?;
        }
        Ok(values)
    }

    async fn check_integrity(&mut self) -> Result<(), Self::Error> {
        let opened = mem::take(&mut self.opened_values);
        let root = self.gen_common_random_element().await?;

        let plain_value = polynomial_eval(opened.iter().map(|x| x.plain_value), root);
        let mac_share = polynomial_eval(opened.iter().map(|x| x.mac_share), root);
        let check_share = mac_share - plain_value * self.dealer().authentication_key_share();

        let shares = self.exchange_with_commitment(check_share).await?;
        let check_plain = shares.into_iter().fold(T::zero(), |acc, x| acc + x);

        if check_plain != T::zero() {
            return Err(SpdzError::Protocol("MAC check failed"));
        }

        // Check consistency of all broadcasts since last check.
        self.state_digest.update(plain_value.to_repr()); // Broadcasts of opened values are included here.
        self.check_state_hash().await
    }
}

impl<T, E, Dealer, Channel> SpdzEngine<T, Dealer, Channel>
where
    T: ff::PrimeField,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = Result<SpdzMessage<T>, E>> + Sink<SpdzMessage<T>> + Unpin,
{
    /// Check if state hashes of all nodes are the same.
    async fn check_state_hash(&mut self) -> Result<(), SpdzError> {
        let state_hash = self.state_digest.finalize_reset().into();
        let msg = SpdzMessage::StateHashCheck(state_hash);
        let received = self.transport.exchange_with_all(msg.clone()).await?;
        if received.into_iter().all(|(_, other_msg)| other_msg == msg) {
            Ok(())
        } else {
            Err(SpdzError::Protocol("State hashes do not match"))
        }
    }

    /// Generate public common random element. Broadcast consistency checking is deferred.
    async fn gen_common_random_element(&mut self) -> Result<T, SpdzError> {
        let seed = T::random(&mut self.rng);
        let all_seeds = self.exchange_with_commitment(seed).await?;
        Ok(all_seeds.into_iter().fold(T::zero(), |acc, x| acc + x))
    }

    /// Commit to field element and exchange them. Broadcast consistency checking is deferred.
    async fn exchange_with_commitment(&mut self, elem: T) -> Result<Vec<T>, SpdzError> {
        let own_salt: [u8; 32] = self.rng.gen();
        let own_hash = SpdzDigest::new()
            .chain_update(own_salt)
            .chain_update(elem.to_repr())
            .finalize()
            .into();

        let received_messages = self
            .transport
            .exchange_with_all(SpdzMessage::Commitment(own_hash))
            .await?;

        let mut all_hashes = vec![Default::default(); self.num_parties()];
        all_hashes[self.party_id()] = own_hash;

        for (other_id, msg) in received_messages {
            if let SpdzMessage::Commitment(other_hash) = msg {
                all_hashes[other_id] = other_hash;
            } else {
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        // Update state digest, we need to check later if broadcast was consistent.
        for hash in &all_hashes {
            self.state_digest.update(hash);
        }

        let received_messages = self
            .transport
            .exchange_with_all(SpdzMessage::RevealCommitment(elem, own_salt))
            .await?;

        let mut all_elems = vec![Default::default(); self.num_parties()];
        all_elems[self.party_id()] = elem;

        for (other_id, msg) in received_messages {
            if let SpdzMessage::RevealCommitment(other_elem, other_salt) = msg {
                let other_hash: SpdzDigestOutput = SpdzDigest::new()
                    .chain_update(other_salt)
                    .chain_update(other_elem.to_repr())
                    .finalize()
                    .into();
                if all_hashes[other_id] != other_hash {
                    return Err(SpdzError::Protocol("Commitment hash mismatch"));
                }
                all_elems[other_id] = other_elem;
            } else {
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        Ok(all_elems)
    }
}

// Evaluate polynomial over field.
fn polynomial_eval<T: ff::Field>(coeffs: impl IntoIterator<Item = T>, x: T) -> T {
    coeffs
        .into_iter()
        .fold(T::zero(), |acc, coeff| acc * x + coeff)
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures::{stream::FuturesUnordered, Future, StreamExt};

    use crate::{
        circuits::{self, join_circuits_all},
        executor::{self, MpcExecutionContext},
        spdz::{FakeSpdzDealer, SpdzShare},
        transport::{self, BincodeDuplex},
    };

    use super::{SpdzEngine, SpdzMessage};

    type Fp = crate::fields::Mersenne127;
    type MockSpdzEngine = SpdzEngine<Fp, FakeSpdzDealer<Fp>, BincodeDuplex<SpdzMessage<Fp>>>;

    async fn run_spdz<F>(inputs: Vec<Vec<Fp>>, circuit_fn: F) -> Vec<Fp>
    where
        F: Copy
            + Fn(
                &'_ MpcExecutionContext<MockSpdzEngine>,
                Vec<Vec<SpdzShare<Fp>>>,
            ) -> Pin<Box<dyn Future<Output = Vec<SpdzShare<Fp>>> + '_>>,
    {
        let num_parties = inputs.len();
        let channel_matrix = transport::mock_multiparty_channels(num_parties, 512);
        let futures = FuturesUnordered::new();

        for (party_id, transport) in channel_matrix.into_iter().enumerate() {
            let dealer = FakeSpdzDealer::new(num_parties, party_id, 123);
            let engine = MockSpdzEngine::new(dealer, transport);
            futures.push(executor::run_circuit(engine, &inputs[party_id], circuit_fn));
        }

        let outputs: Vec<_> = futures.map(|result| result.unwrap()).collect().await;
        for i in 1..num_parties {
            assert_eq!(outputs[i], outputs[0], "Mismatched outputs",);
        }
        outputs.into_iter().next().unwrap()
    }

    #[tokio::test]
    async fn test_spdz() {
        let outputs = run_spdz(
            vec![
                vec![1.into(), 2.into(), 3.into()],
                vec![4.into(), 5.into(), 6.into()],
                vec![7.into(), 8.into(), 9.into()],
            ],
            |ctx, inputs| {
                Box::pin(async move {
                    let num_elems = inputs[0].len();
                    join_circuits_all((0..num_elems).map(|i| {
                        circuits::elementary::product(ctx, inputs.iter().map(move |x| x[i]))
                    }))
                    .await
                })
            },
        )
        .await;
        assert_eq!(outputs, vec![28.into(), 80.into(), 162.into()]);
    }
}
