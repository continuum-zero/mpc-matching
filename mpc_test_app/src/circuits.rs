use std::{fmt, iter};

use mpc::{
    circuits::{join_circuits_all, IntShare, WrappedShare},
    executor::{self, MpcExecutionContext, MpcExecutionError, MpcExecutionStats},
    ff::Field,
    MpcEngine, MpcField,
};
use mpc_flow::FlowError;
use ndarray::Array2;

/// Error during private matching computation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchingError<E> {
    Execution(MpcExecutionError<E>),
    Flow(FlowError),
    InvalidOutput,
    InputLengthMismatch,
}

impl<E: fmt::Display> fmt::Display for MatchingError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Execution(ref err) => err.fmt(f),
            Self::Flow(ref err) => err.fmt(f),
            Self::InvalidOutput => write!(f, "Output from circuit was invalid"),
            Self::InputLengthMismatch => write!(f, "Parties have inconsistent input lengths"),
        }
    }
}

impl<E> From<MpcExecutionError<E>> for MatchingError<E> {
    fn from(err: MpcExecutionError<E>) -> Self {
        MatchingError::Execution(err)
    }
}

impl<E> From<FlowError> for MatchingError<E> {
    fn from(err: FlowError) -> Self {
        MatchingError::Flow(err)
    }
}

/// Vector of party preferences.
pub type PreferenceVec = Vec<u64>;

/// Given preferences of 2N parties, find matching between parties 0, ..., n-1 and parties n, ..., 2n-1,
/// such that total cost is minimum possible. Total cost is sum of costs of individual pairs.
/// Cost of pair is square of L2 distance between preference vectors.
/// Returns index of party matched to current party.
pub async fn compute_private_matching<Engine, Error, const N: usize>(
    engine: Engine,
    preferences: PreferenceVec,
    max_preference_value: u64,
) -> Result<(usize, MpcExecutionStats), MatchingError<Error>>
where
    Engine: 'static + Send + MpcEngine<Error = Error>,
    Error: 'static + Send,
{
    if engine.num_parties() % 2 != 0 {
        panic!("Protocol requires even number of parties")
    }

    let num_parties = engine.num_parties();
    let party_id = engine.party_id();

    // Random field element that is used to hide our output from circuit.
    let output_mask = Engine::Field::random(&mut rand::thread_rng());

    // Input to circuit is concatenation of [output_mask] and preference vector.
    let inputs: Vec<_> = iter::once(output_mask)
        .chain(preferences.into_iter().map(Engine::Field::from))
        .collect();

    let (circuit_output, stats) =
        executor::run_circuit_in_background(engine, inputs, move |ctx, inputs| {
            Box::pin(matching_circuit::<_, N>(ctx, inputs, max_preference_value))
        })
        .await?;

    let outputs = circuit_output?;

    // Extract our output and "decrypt" it by subtracting mask.
    let output = (outputs[party_id] - output_mask).truncated() as usize;

    if output < num_parties {
        Ok((output as usize, stats))
    } else {
        Err(MatchingError::InvalidOutput)
    }
}

/// Circuit used by `compute_private_matching`. Computes matching and returns masked outputs of all parties.
async fn matching_circuit<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    inputs: Vec<Vec<E::Share>>,
    max_preference_value: u64,
) -> Result<Vec<E::Field>, MatchingError<E::Error>> {
    if !inputs.iter().all(|x| x.len() == inputs[0].len()) {
        return Err(MatchingError::InputLengthMismatch);
    }

    let max_preference_value = IntShare::from_plain(ctx, max_preference_value as i64);

    // The first input of each party is its output mask.
    let output_masks = inputs.iter().map(|vec| vec[0]);

    // The rest of inputs form preference vectors.
    let preferences: Vec<_> = join_circuits_all(inputs.iter().map(|vec| {
        join_circuits_all(vec[1..].iter().map(|&x| {
            IntShare::<_, N>::wrap_clamped(ctx, x, IntShare::zero(), max_preference_value)
        }))
    }))
    .await;

    let first_right_index = preferences.len() / 2;
    let left_preferences = &preferences[..first_right_index];
    let right_preferences = &preferences[first_right_index..];

    let costs = get_cost_matrix(ctx, left_preferences, right_preferences).await;

    let (left_matches, right_matches) =
        mpc_flow::min_cost_bipartite_matching(ctx, costs.view()).await?;

    let matches = left_matches
        .into_iter()
        .map(|x| x + IntShare::from_plain(ctx, first_right_index as i64))
        .chain(right_matches);

    let masked_matches = matches
        .zip(output_masks)
        .map(|(value, mask)| value.raw() + mask);

    ctx.ensure_integrity();
    Ok(join_circuits_all(masked_matches.map(|x| ctx.open_unchecked(x))).await)
}

/// Compute matrix of costs for each possible pair.
async fn get_cost_matrix<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    left_preferences: &[Vec<IntShare<E::Share, N>>],
    right_preferences: &[Vec<IntShare<E::Share, N>>],
) -> Array2<IntShare<E::Share, N>> {
    let costs = join_circuits_all(left_preferences.iter().flat_map(|left| {
        right_preferences
            .iter()
            .map(|right| compare_preferences(ctx, left, right))
    }))
    .await;
    Array2::from_shape_vec((left_preferences.len(), right_preferences.len()), costs).unwrap()
}

/// Returns square of L2 distance between preference vectors.
async fn compare_preferences<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    left: &[IntShare<E::Share, N>],
    right: &[IntShare<E::Share, N>],
) -> IntShare<E::Share, N> {
    join_circuits_all(left.iter().zip(right).map(|(&x, &y)| {
        let delta = x - y;
        delta.mul(ctx, delta)
    }))
    .await
    .into_iter()
    .fold(IntShare::zero(), |acc, x| acc + x)
}
