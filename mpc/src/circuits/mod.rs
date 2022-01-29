pub mod elementary;

/// Wait on multiple concurrent branches, returning when **all** branches complete.
/// This macro guarantees deterministic polling order of provided futurs,
/// which makes it safe to use with our circuits.
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! join_circuits {
    // TODO: Currently we rely on the fact that futures::join! polls futures in the same order each time.
    // This is undocumented, so we shouldn't rely on this.
    ($($tokens:tt)*) => {{
        futures::join!($( $tokens )*)
    }}
}
