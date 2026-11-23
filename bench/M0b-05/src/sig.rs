//! SIGINT and SIGTERM set a flag that the main loop reads; the spike then stops the
//! capture and exits 0.

use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
}

extern "C" fn on_signal(_sig: i32) {
    STOP.store(true, Ordering::SeqCst);
}

/// Install the handlers.
pub fn install() {
    // SAFETY: `on_signal` only stores to an atomic, which is async-signal-safe. 2 = SIGINT, 15 = SIGTERM.
    unsafe {
        signal(2, on_signal as extern "C" fn(i32) as usize);
        signal(15, on_signal as extern "C" fn(i32) as usize);
    }
}

/// True once SIGINT or SIGTERM arrived.
pub fn stopped() -> bool {
    STOP.load(Ordering::SeqCst)
}
