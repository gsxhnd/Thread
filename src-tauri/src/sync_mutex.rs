//! Mutex helpers that recover from poison instead of panicking.
//!
//! Mutex 辅助函数：遇到 poison 时恢复内部数据，避免 panic。

use std::sync::{Mutex, MutexGuard};

pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
