use std::sync::{Mutex, MutexGuard};

static NETWORK_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock_network_test() -> MutexGuard<'static, ()> {
    NETWORK_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
