mod basic;
pub use basic::*;

mod bitwise;
pub use bitwise::*;

mod boolean;
pub use boolean::*;

mod integer;
pub use integer::*;

mod sequences;
pub use sequences::*;

use std::{future::Future, pin::Pin, task::Poll};

pub use futures; // Reexport futures crate for join_circuits! macro.

/// Wait on multiple concurrent branches, returning when all branches complete.
/// This is a variant of futures::join! macro, that guarantees deterministic polling order,
/// which makes it safe to use with our async circuits.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! join_circuits {
    // TODO: Currently we rely on the fact that futures::join! polls futures in the same order each time.
    // This is undocumented, so we should roll our own implementation.
    ($($tokens:tt)*) => {{
        $crate::circuits::futures::join!($( $tokens )*)
    }}
}

/// Wait on multiple concurrent branches, returning when all branches complete.
/// This is a variant of futures::join_all function, that guarantees deterministic polling order,
/// which makes it safe to use with our async circuits.
pub async fn join_circuits_all<T, F>(futures: impl IntoIterator<Item = F>) -> Vec<T>
where
    F: Future<Output = T>,
{
    let mut futures: Pin<Box<_>> = futures
        .into_iter()
        .map(|f| futures::future::maybe_done(f))
        .collect::<Box<_>>()
        .into();

    futures::future::poll_fn(move |ctx| {
        let mut is_done = true;

        for fut in iter_pin_mut(futures.as_mut()) {
            if fut.poll(ctx).is_pending() {
                is_done = false;
            }
        }

        if is_done {
            Poll::Ready(
                iter_pin_mut(futures.as_mut())
                    .map(|fut| fut.take_output().unwrap())
                    .collect(),
            )
        } else {
            Poll::Pending
        }
    })
    .await
}

// Taken from: https://github.com/rust-lang/futures-rs/blob/master/futures-util/src/future/join_all.rs#L18-L23
fn iter_pin_mut<T>(slice: Pin<&mut [T]>) -> impl Iterator<Item = Pin<&mut T>> {
    unsafe { slice.get_unchecked_mut() }
        .iter_mut()
        .map(|t| unsafe { Pin::new_unchecked(t) })
}

pub mod testing {
    use futures::FutureExt;
    use std::future::Future;
    use std::pin::Pin;

    /// Field for circuits tests.
    pub type MockField = crate::fields::Mersenne127;

    /// Fake MPC engine for circuits tests.
    pub type MockEngine = crate::plaintext::PlainMpcEngine<MockField>;

    /// Execution context for circuits tests.
    pub type MockExecutionContext = crate::executor::MpcExecutionContext<MockEngine>;

    /// Test async circuit in mock plaintext environment.
    pub async fn test_circuit<F>(circuit_fn: F)
    where
        F: FnOnce(&'_ MockExecutionContext) -> Pin<Box<dyn Future<Output = ()> + '_>>,
    {
        crate::executor::run_circuit(MockEngine::new(), &[], |ctx, _| {
            let future = circuit_fn(ctx);
            Box::pin(future.map(|_| vec![]))
        })
        .await
        .unwrap();
    }
}
