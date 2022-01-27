use std::{
    cell::{Cell, RefCell},
    future::Future,
    mem,
    pin::Pin,
    task::Poll,
};

use crate::*;

/// MPC async circuit executor.
pub struct MpcExecutor<Engine: MpcEngine> {
    engine: Engine,
}

impl<Engine: MpcEngine> MpcExecutor<Engine> {
    /// Create new instance.
    pub fn new(engine: Engine) -> Self {
        MpcExecutor { engine }
    }

    /// Execute async circuit.
    pub async fn run<F>(self, circuit_fn: F)
    where
        F: FnOnce(&'_ MpcExecutionContext<Engine>) -> Pin<Box<dyn Future<Output = ()> + '_>>,
    {
        let ctx = MpcExecutionContext::new(self.engine);
        let mut future = circuit_fn(&ctx);

        while futures::poll!(future.as_mut()).is_pending() {
            let requests = ctx.open_buffer.take_requests();
            if requests.is_empty() {
                panic!("Circuit didn't make progress");
            }

            let responses = ctx.engine.process_openings_bundle(requests).await;
            ctx.open_buffer.resolve_all(responses);
        }
    }
}

/// MPC async circuit execution context.
pub struct MpcExecutionContext<Engine: MpcEngine> {
    engine: Engine,
    open_buffer: RoundCommandBuffer<Engine::Share, Engine::Field>,
}

impl<Engine: MpcEngine> MpcExecutionContext<Engine> {
    /// Create new MPC circuit executor.
    pub fn new(engine: Engine) -> Self {
        MpcExecutionContext {
            engine,
            open_buffer: RoundCommandBuffer::new(),
        }
    }

    /// Get dealer associated with this computation.
    pub fn dealer(&self) -> &Engine::Dealer {
        self.engine.dealer()
    }

    /// Open provided share. Requires communication.
    /// Warning: Integrity checks may be deferred to output phase (like in SPDZ protocol). Use with care.
    pub async fn partial_open(&self, input: Engine::Share) -> Engine::Field {
        self.open_buffer.queue(input).await
    }
}

impl<Engine: MpcEngine> MpcContext for MpcExecutionContext<Engine> {
    type Field = Engine::Field;
    type Share = Engine::Share;

    fn num_parties(&self) -> usize {
        self.engine.num_parties()
    }

    fn party_id(&self) -> usize {
        self.engine.party_id()
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
        let index = self.requests.borrow().len();
        let pending_round = self.round_index.get();
        let ready_round = self.round_index.get().wrapping_add(1);
        self.requests.borrow_mut().push(input);

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
