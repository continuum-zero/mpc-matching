use std::{
    cell::{Cell, RefCell, RefMut},
    future::Future,
    mem,
    pin::Pin,
    task::Poll,
    thread,
};

use tokio::sync::oneshot;

use crate::{MpcContext, MpcDealer, MpcEngine, MpcShare};

/// MPC async circuit execution context.
pub struct MpcExecutionContext<Engine: MpcEngine> {
    engine: RefCell<Engine>,
    open_buffer: RoundCommandBuffer<Engine::Share, Engine::Field>,
    force_integrity_check: Cell<bool>,
    cached_one: Engine::Share,
    cached_two: Engine::Share,
}

impl<Engine: MpcEngine> MpcExecutionContext<Engine> {
    /// Create new MPC circuit executor.
    fn new(mut engine: Engine) -> Self {
        let one = engine.dealer().share_plain(ff::Field::one());
        MpcExecutionContext {
            engine: RefCell::new(engine),
            open_buffer: RoundCommandBuffer::new(),
            force_integrity_check: Cell::new(false),
            cached_one: one,
            cached_two: one.double(),
        }
    }

    /// Get underlying MPC engine.
    pub fn engine(&self) -> RefMut<Engine> {
        self.engine.borrow_mut()
    }

    /// Open provided share. Requires communication.
    /// Warning: Integrity checks may be deferred (like in SPDZ protocol). Use with care.
    pub async fn open_unchecked(&self, input: Engine::Share) -> Engine::Field {
        self.open_buffer.queue(input).await
    }

    /// Ensure integrity of everything computed so far.
    /// The check will be executed at the beginning of next round.
    pub fn ensure_integrity(&self) {
        self.force_integrity_check.set(true);
    }

    /// Cached sharing of one.
    pub fn one(&self) -> Engine::Share {
        self.cached_one
    }

    /// Cached sharing of two.
    pub fn two(&self) -> Engine::Share {
        self.cached_two
    }

    /// Sharing of plaintext element.
    pub fn plain(&self, value: Engine::Field) -> Engine::Share {
        self.cached_one * value
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
pub async fn run_circuit<Engine, F, T>(
    mut engine: Engine,
    inputs: &[Engine::Field],
    circuit_fn: F,
) -> Result<T, Engine::Error>
where
    Engine: MpcEngine,
    F: FnOnce(
        &'_ MpcExecutionContext<Engine>,
        Vec<Vec<Engine::Share>>,
    ) -> Pin<Box<dyn Future<Output = T> + '_>>,
{
    let input_shares = engine
        .process_inputs(inputs.iter().copied().collect())
        .await?;

    let ctx = MpcExecutionContext::new(engine);
    let mut future = circuit_fn(&ctx, input_shares);

    loop {
        if let Poll::Ready(outputs) = futures::poll!(future.as_mut()) {
            ctx.engine().check_integrity().await?;
            return Ok(outputs);
        }

        if ctx.force_integrity_check.get() {
            ctx.engine().check_integrity().await?;
            ctx.force_integrity_check.set(false);
        }

        let requests = ctx.open_buffer.take_requests();
        if requests.is_empty() {
            panic!("Circuit didn't make progress");
        }

        let responses = ctx.engine().process_openings_unchecked(requests).await?;
        ctx.open_buffer.resolve_all(responses);
    }
}

/// Execute async circuit on a dedicated thread.
pub async fn run_circuit_in_background<Engine, Error, F, T>(
    engine: Engine,
    inputs: Vec<Engine::Field>,
    circuit_fn: F,
) -> Result<T, Engine::Error>
where
    Engine: 'static + Send + MpcEngine<Error = Error>,
    Error: 'static + Send,
    T: 'static + Send,
    F: 'static
        + Send
        + FnOnce(
            &'_ MpcExecutionContext<Engine>,
            Vec<Vec<Engine::Share>>,
        ) -> Pin<Box<dyn Future<Output = T> + '_>>,
{
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let future = run_circuit(engine, &inputs, circuit_fn);
        let result = runtime.block_on(future);
        let _ = sender.send(result);
    });
    receiver.await.unwrap()
}

/// Buffer for accumulating commands issued by async circuit.
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
        mem::take(&mut self.requests.borrow_mut())
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
        responses.extend(new_responses.into_iter().map(Some));
        self.round_index.set(self.round_index.get().wrapping_add(1));
        self.first_unpolled_response.set(0);
    }
}
