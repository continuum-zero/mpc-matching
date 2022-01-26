use std::ops::{Add, Mul};

use async_trait::async_trait;

pub mod circuits;
pub mod executor;
pub mod plaintext;

/// Private share of a field element.
/// Sharing is linear and supports addition with plaintext field elements without communication.
pub trait MpcShare:
    Copy
    + Clone
    + Add<Output = Self>
    + Add<Self::Field, Output = Self>
    + Mul<Self::Field, Output = Self>
{
    /// Field type of value represented by this share.
    type Field: Copy + Clone + Add<Output = Self::Field> + Mul<Output = Self::Field>;
}

/// MPC computation info.
pub trait MpcContext {
    /// Field type used by this MPC protocol.
    type Field: Copy + Clone + Add<Output = Self::Field> + Mul<Output = Self::Field>;

    /// Share type used by this MPC protocol.
    type Share: MpcShare<Field = Self::Field>;

    /// Number of parties participating in MPC computation.
    fn num_parties(&self) -> usize;

    /// ID of current party.
    fn party_id(&self) -> usize;
}

/// Low-level interface of sharing-based MPC protocol.
#[async_trait]
pub trait MpcEngine: MpcContext {
    async fn process_round(&self, requests: MpcRoundInput<Self>) -> MpcRoundOutput<Self>;
}

/// Bundle of requests for a single MPC round.
pub struct MpcRoundInput<E: MpcContext + ?Sized> {
    pub input_requests: Vec<InputRequest<E>>,
    pub mul_requests: Vec<MulRequest<E>>,
    pub open_requests: Vec<OpenRequest<E>>,
}

/// Bundle of responses from a single MPC round.
pub struct MpcRoundOutput<E: MpcContext + ?Sized> {
    pub input_responses: Vec<InputResponse<E>>,
    pub mul_responses: Vec<MulResponse<E>>,
    pub open_responses: Vec<OpenResponse<E>>,
}

#[derive(Clone, Copy)]
pub struct InputRequest<E: MpcContext + ?Sized> {
    pub owner: usize,
    pub value: Option<E::Field>,
}

#[derive(Clone, Copy)]
pub struct InputResponse<E: MpcContext + ?Sized>(E::Share);

#[derive(Clone, Copy)]
pub struct MulRequest<E: MpcContext + ?Sized>(E::Share, E::Share);

#[derive(Clone, Copy)]
pub struct MulResponse<E: MpcContext + ?Sized>(E::Share);

#[derive(Clone, Copy)]
pub struct OpenRequest<E: MpcContext + ?Sized>(E::Share);

#[derive(Clone, Copy)]
pub struct OpenResponse<E: MpcContext + ?Sized>(E::Field);
