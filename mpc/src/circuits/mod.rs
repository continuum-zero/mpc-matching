pub mod elementary;

pub use futures; // Reexport futures crate for join_circuits! macro.

/// Wait on multiple concurrent branches, returning when **all** branches complete.
/// This macro guarantees deterministic polling order of provided futures,
/// which makes it safe to use with our async circuits.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! join_circuits {
    // TODO: Currently we rely on the fact that futures::join! polls futures in the same order each time.
    // This is undocumented, so we shouldn't rely on this.
    ($($tokens:tt)*) => {{
        $crate::circuits::futures::join!($( $tokens )*)
    }}
}
