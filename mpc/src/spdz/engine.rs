use std::{fmt, mem};

use async_trait::async_trait;
use digest::Digest;
use futures::{Sink, Stream};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{
    transport::{MultipartyTransport, TransportError},
    MpcContext, MpcEngine, MpcField,
};

use super::{SpdzDealer, SpdzDigest, SpdzDigestOutput, SpdzRng, SpdzShare};

/// Threshold of collected partial openings that triggers integrity checking.
const BATCH_CHECK_THRESHOLD: usize = 20000;

/// Maximum number of values that can be checked using a single polynomial hash.
const MAX_BATCH_CHECK_SIZE: usize = 40000;

/// Salt for commitments.
type CommitmentSalt = [u8; 32];

/// SPDZ protocol message.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum SpdzMessage<T> {
    MaskedInputs(Vec<T>),
    SharesExchange(Vec<T>),
    ShareSumExchange(Vec<T>),
    StateHashCheck(SpdzDigestOutput),
    Commitment(SpdzDigestOutput),
    Decommitment(T, CommitmentSalt),
}

/// SPDZ error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpdzError {
    Transport(TransportError),
    UnexpectedMessage(usize),
    IncorrectNumberOfValues(usize),
    CommitmentHashMismatch(usize),
    StateHashMismatch,
    MacCheckFailed,
}

impl fmt::Display for SpdzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Transport(ref inner) => inner.fmt(f),
            Self::UnexpectedMessage(id) => write!(f, "Received unexpected message from {}", id),
            Self::IncorrectNumberOfValues(id) => {
                write!(f, "Received incorrect number of values from {}", id)
            }
            Self::CommitmentHashMismatch(id) => {
                write!(f, "Commitment hash doesn't match reveal value for {}", id)
            }
            Self::StateHashMismatch => write!(f, "State hashes do not match"),
            Self::MacCheckFailed => write!(f, "MAC check failed"),
        }
    }
}

impl From<TransportError> for SpdzError {
    fn from(err: TransportError) -> Self {
        SpdzError::Transport(err)
    }
}

/// Saved opened value for batch MAC checking.
struct PartiallyOpenedValue<T> {
    plain_value: T,
    mac_share: T,
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
    T: MpcField,
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
    T: MpcField,
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
                return Err(SpdzError::UnexpectedMessage(other_id));
            }
        }

        for deltas in all_deltas {
            self.state_digest.update(deltas.len().to_le_bytes());
            for x in deltas {
                self.state_digest.update(x.to_repr());
            }
        }
        self.check_state_hashes().await?;
        Ok(all_shares)
    }

    async fn process_openings_unchecked(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, SpdzError> {
        let mut values: Vec<_> = requests.iter().map(|x| x.value).collect();
        let values_count = values.len();

        if self.party_id() == 0 {
            for (other_id, msg) in self.transport.receive_from_all().await? {
                if let SpdzMessage::SharesExchange(parts) = msg {
                    if parts.len() != values_count {
                        return Err(SpdzError::IncorrectNumberOfValues(other_id));
                    }
                    for (i, part) in parts.into_iter().enumerate() {
                        values[i] += part;
                    }
                } else {
                    return Err(SpdzError::UnexpectedMessage(other_id));
                }
            }
            self.transport
                .send_to_all(SpdzMessage::ShareSumExchange(values.clone()))
                .await?;
        } else {
            self.transport
                .send_to(0, SpdzMessage::SharesExchange(values))
                .await?;
            if let SpdzMessage::ShareSumExchange(sums) = self.transport.receive_from(0).await? {
                if sums.len() != values_count {
                    return Err(SpdzError::IncorrectNumberOfValues(0));
                }
                values = sums;
            } else {
                return Err(SpdzError::UnexpectedMessage(0));
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

        if self.opened_values.len() >= BATCH_CHECK_THRESHOLD {
            self.check_integrity().await?;
        }
        Ok(values)
    }

    async fn check_integrity(&mut self) -> Result<(), Self::Error> {
        let opened_values = mem::take(&mut self.opened_values);

        for chunk in opened_values.chunks(MAX_BATCH_CHECK_SIZE) {
            let root = self.gen_common_random_element().await?;

            let plain_value = polynomial_eval(chunk.iter().map(|x| x.plain_value), root);
            let mac_share = polynomial_eval(chunk.iter().map(|x| x.mac_share), root);
            let check_share = mac_share - plain_value * self.dealer().authentication_key_share();

            let shares = self.exchange_with_commitment(check_share).await?;
            let check_plain = shares.into_iter().fold(T::zero(), |acc, x| acc + x);

            if check_plain != T::zero() {
                return Err(SpdzError::MacCheckFailed);
            }

            // Ensure broadcasted values were consistent by including their combination in state hash.
            self.state_digest.update(plain_value.to_repr());
        }

        // Check consistency of all broadcasts since last check.
        self.check_state_hashes().await
    }
}

impl<T, E, Dealer, Channel> SpdzEngine<T, Dealer, Channel>
where
    T: MpcField,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = Result<SpdzMessage<T>, E>> + Sink<SpdzMessage<T>> + Unpin,
{
    /// Check if state hashes of all nodes are the same.
    async fn check_state_hashes(&mut self) -> Result<(), SpdzError> {
        let state_hash = self.state_digest.finalize_reset().into();
        let msg = SpdzMessage::StateHashCheck(state_hash);
        let received = self.transport.exchange_with_all(msg.clone()).await?;
        if received.into_iter().all(|(_, other_msg)| other_msg == msg) {
            Ok(())
        } else {
            Err(SpdzError::StateHashMismatch)
        }
    }

    /// Generate public common random element. Broadcast consistency checking is deferred.
    async fn gen_common_random_element(&mut self) -> Result<T, SpdzError> {
        let seed = T::random(&mut self.rng);
        let all_seeds = self.exchange_with_commitment(seed).await?;
        Ok(all_seeds.into_iter().fold(T::zero(), |acc, x| acc + x))
    }

    /// Commit to field elements and exchange them. Broadcast consistency checking is deferred.
    async fn exchange_with_commitment(&mut self, elem: T) -> Result<Vec<T>, SpdzError> {
        let own_salt: CommitmentSalt = self.rng.gen();
        let own_hash = commit_value(elem, own_salt);

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
                return Err(SpdzError::UnexpectedMessage(other_id));
            }
        }

        // Update state digest, we need to ensure later if broadcasts were consistent.
        for hash in &all_hashes {
            self.state_digest.update(hash);
        }

        let received_messages = self
            .transport
            .exchange_with_all(SpdzMessage::Decommitment(elem, own_salt))
            .await?;

        let mut all_elems = vec![Default::default(); self.num_parties()];
        all_elems[self.party_id()] = elem;

        for (other_id, msg) in received_messages {
            if let SpdzMessage::Decommitment(other_elem, other_salt) = msg {
                let other_hash = commit_value(other_elem, other_salt);
                if all_hashes[other_id] != other_hash {
                    return Err(SpdzError::CommitmentHashMismatch(other_id));
                }
                all_elems[other_id] = other_elem;
            } else {
                return Err(SpdzError::UnexpectedMessage(other_id));
            }
        }

        Ok(all_elems)
    }
}

/// Commit to a value with given salt.
fn commit_value<T: MpcField>(value: T, salt: CommitmentSalt) -> SpdzDigestOutput {
    SpdzDigest::new()
        .chain_update(salt)
        .chain_update(value.to_repr())
        .finalize()
        .into()
}

// Evaluate polynomial over field.
fn polynomial_eval<T: ff::Field>(coeffs: impl IntoIterator<Item = T>, x: T) -> T {
    coeffs
        .into_iter()
        .fold(T::zero(), |acc, coeff| acc * x + coeff)
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
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

    async fn run_spdz<F, T>(inputs: Vec<Vec<Fp>>, circuit_fn: F) -> T
    where
        T: PartialEq + Eq + Debug,
        F: Copy
            + Fn(
                &'_ MpcExecutionContext<MockSpdzEngine>,
                Vec<Vec<SpdzShare<Fp>>>,
            ) -> Pin<Box<dyn Future<Output = T> + '_>>,
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
        outputs.into_iter().next().unwrap().0
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
                    join_circuits_all(
                        (0..num_elems)
                            .map(|i| circuits::product(ctx, inputs.iter().map(move |x| x[i])))
                            .map(|share_future| async move {
                                let share = share_future.await;
                                ctx.ensure_integrity();
                                ctx.open_unchecked(share).await
                            }),
                    )
                    .await
                })
            },
        )
        .await;
        assert_eq!(outputs, vec![28.into(), 80.into(), 162.into()]);
    }
}
