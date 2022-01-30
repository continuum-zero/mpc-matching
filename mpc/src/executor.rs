use std::{
    cell::{Cell, RefCell, RefMut},
    future::Future,
    mem,
    pin::Pin,
    task::Poll,
};

use crate::*;

/// MPC async circuit execution context.
pub struct MpcExecutionContext<Engine: MpcEngine> {
    engine: RefCell<Engine>,
    open_buffer: RoundCommandBuffer<Engine::Share, Engine::Field>,
}

impl<Engine: MpcEngine> MpcExecutionContext<Engine> {
    /// Create new MPC circuit executor.
    pub fn new(engine: Engine) -> Self {
        MpcExecutionContext {
            engine: RefCell::new(engine),
            open_buffer: RoundCommandBuffer::new(),
        }
    }

    /// Get underlying MPC engine.
    pub fn engine(&self) -> RefMut<Engine> {
        self.engine.borrow_mut()
    }

    /// Open provided share. Requires communication.
    /// Warning: Integrity checks may be deferred to output phase (like in SPDZ protocol). Use with care.
    pub async fn open_unchecked(&self, input: Engine::Share) -> Engine::Field {
        self.open_buffer.queue(input).await
    }
}

impl<Engine: MpcEngine> MpcContext for MpcExecutionContext<Engine> {
    type Field = Engine::Field;
    type Share = Engine::Share;

    fn num_parties(&self) -> usize {
        self.engine().num_parties()
    }

    fn party_id(&self) -> usize {
        self.engine().party_id()
    }
}

/// Execute async circuit.
pub async fn run_circuit<Engine, F>(
    mut engine: Engine,
    inputs: &[Engine::Field],
    circuit_fn: F,
) -> Vec<Engine::Field>
where
    Engine: MpcEngine,
    F: FnOnce(
        &'_ MpcExecutionContext<Engine>,
        Vec<Vec<Engine::Share>>,
    ) -> Pin<Box<dyn Future<Output = Vec<Engine::Share>> + '_>>,
{
    let input_shares = engine
        .process_inputs(inputs.iter().copied().collect())
        .await;

    let ctx = MpcExecutionContext::new(engine);
    let mut future = circuit_fn(&ctx, input_shares);

    loop {
        if let Poll::Ready(shares_to_open) = futures::poll!(future.as_mut()) {
            return ctx.engine().process_outputs(shares_to_open).await;
        }

        let requests = ctx.open_buffer.take_requests();
        if requests.is_empty() {
            panic!("Circuit didn't make progress");
        }

        let responses = ctx.engine().process_openings_unchecked(requests).await;
        ctx.open_buffer.resolve_all(responses);
    }
}

/// Buffer for accumulating commands issues by async circuit.
struct RoundCommandBuffer<T, S> {
    requests: RefCell<Vec<T>>,
    responses: RefCell<Vec<Option<S>>>,
    round_index: Cell<usize>,
    first_unpolled_response: Cell<usize>,
}

impl<T, S> RoundCommandBuffer<T, S> {
    /// Create new instance.
    fn new() -> Self {
        RoundCommandBuffer {
            requests: RefCell::new(Vec::new()),
            responses: RefCell::new(Vec::new()),
            round_index: Cell::new(0),
            first_unpolled_response: Cell::new(0),
        }
    }

    /// Queue new command and asynchronously wait for response.
    async fn queue(&self, input: T) -> S {
        let pending_round = self.round_index.get();
        let ready_round = pending_round.wrapping_add(1);
        let index = {
            let mut requests = self.requests.borrow_mut();
            requests.push(input);
            requests.len() - 1
        };

        futures::future::poll_fn(|_| {
            if self.round_index.get() == ready_round {
                if self.first_unpolled_response.get() != index {
                    panic!("Circuit execution went out of order");
                }
                self.first_unpolled_response.set(index + 1);
                Poll::Ready(
                    self.responses.borrow_mut()[index]
                        .take()
                        .expect("Future polled twice"),
                )
            } else {
                if self.round_index.get() != pending_round {
                    panic!("Circuit execution went out of order");
                }
                Poll::Pending
            }
        })
        .await
    }

    /// Take requests accumulated during last round.
    fn take_requests(&self) -> Vec<T> {
        mem::replace(&mut self.requests.borrow_mut(), Vec::new())
    }

    /// Resolve all requests issued during last round.
    fn resolve_all(&self, new_responses: impl IntoIterator<Item = S>) {
        let mut requests = self.requests.borrow_mut();
        let mut responses = self.responses.borrow_mut();

        if self.first_unpolled_response.get() != responses.len() {
            panic!("Some responses from previous round were not processed");
        }

        requests.clear();
        responses.clear();
        responses.extend(new_responses.into_iter().map(|x| Some(x)));
        self.round_index.set(self.round_index.get().wrapping_add(1));
        self.first_unpolled_response.set(0);
    }
}
