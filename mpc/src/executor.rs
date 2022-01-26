use std::{
    cell::{Cell, RefCell},
    future::Future,
    mem,
    task::Poll,
};

use crate::*;

/// MPC circuit executor.
pub struct MpcExecutor<'a, Engine: MpcEngine> {
    engine: &'a Engine,
    input_buffer: RoundCommandBuffer<InputRequest<Engine>, InputResponse<Engine>>,
    mul_buffer: RoundCommandBuffer<MulRequest<Engine>, MulResponse<Engine>>,
    open_buffer: RoundCommandBuffer<OpenRequest<Engine>, OpenResponse<Engine>>,
}

impl<'a, Engine: MpcEngine> MpcExecutor<'a, Engine> {
    /// Create new MPC circuit executor.
    pub fn new(engine: &'a Engine) -> Self {
        MpcExecutor {
            engine,
            input_buffer: RoundCommandBuffer::new(),
            mul_buffer: RoundCommandBuffer::new(),
            open_buffer: RoundCommandBuffer::new(),
        }
    }

    /// Get private share of next input value provided by party `owner`.
    /// If `party_id() != owner`, then `value` must be None.
    /// If `party_id() == owner`, then `value` must contain input value.
    pub async fn input(&self, owner: usize, value: Option<Engine::Field>) -> Engine::Share {
        self.input_buffer
            .queue(InputRequest { owner, value })
            .await
            .0
    }

    /// Multiply shared values. Requires communication.
    pub async fn mul(&self, a: Engine::Share, b: Engine::Share) -> Engine::Share {
        self.mul_buffer.queue(MulRequest(a, b)).await.0
    }

    /// Open provided share. Requires communication.
    pub async fn open(&self, a: Engine::Share) -> Engine::Field {
        self.open_buffer.queue(OpenRequest(a)).await.0
    }

    /// Execute given async circuit on specified MPC engine.
    pub async fn run_circuit<T, S, F>(&self, circuit: F) -> T
    where
        S: Future<Output = T>,
        F: FnOnce() -> S,
    {
        let mut future = Box::pin(circuit());

        loop {
            if let Poll::Ready(output) = futures::poll!(future.as_mut()) {
                return output;
            }

            let requests = MpcRoundInput {
                input_requests: self.input_buffer.take_requests(),
                mul_requests: self.mul_buffer.take_requests(),
                open_requests: self.open_buffer.take_requests(),
            };

            let responses = self.engine.process_round(requests).await;
            self.input_buffer.resolve_all(responses.input_responses);
            self.mul_buffer.resolve_all(responses.mul_responses);
            self.open_buffer.resolve_all(responses.open_responses);
        }
    }
}

impl<'a, Engine: MpcEngine> MpcContext for MpcExecutor<'a, Engine> {
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
        let target_round = self.round_index.get() + 1;
        self.requests.borrow_mut().push(input);

        futures::future::poll_fn(|_| {
            if self.round_index.get() == target_round {
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
                Poll::Pending
            }
        })
        .await
    }

    /// Get requests accumulated during last round.
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
        self.round_index.set(self.round_index.get() + 1);
        self.first_unpolled_response.set(0);
    }
}
