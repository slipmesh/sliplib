//! A test binary of its own: the subscriber is global to the process.

#[test]
fn installs_the_global_subscriber() {
    sliplib_runtime::init_tracing();
    assert!(tracing::dispatcher::has_been_set());
}
