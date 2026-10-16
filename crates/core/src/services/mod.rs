//! Service layer: protocol clients, history, variables, secrets.

use std::future::Future;

use parking_lot::Mutex;
use tokio::runtime::Handle;

use crate::error::{AppError, AppResult};

pub mod graphql;
pub mod grpc;
pub mod history;
pub mod http;
pub mod proxy;
pub mod secrets;
mod transport;
pub mod variables;
pub mod websocket;

/// Spawns work on the Tokio runtime that is already entered (tests and tasks
/// started from the core runtime), or on the handle [`bind_io`] stored for the
/// UI thread, which has no reactor of its own.
pub(crate) fn spawn_io<F>(
    bound: &Mutex<Option<Handle>>,
    future: F,
) -> AppResult<tokio::task::JoinHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handle = Handle::try_current().ok().or_else(|| bound.lock().clone());
    match handle {
        Some(handle) => Ok(handle.spawn(future)),
        None => Err(AppError::Internal(
            "tokio runtime is not available on this thread".into(),
        )),
    }
}

/// Remembers the application runtime so later calls from the UI thread can spawn.
pub(crate) fn bind_io(slot: &Mutex<Option<Handle>>, handle: Handle) {
    *slot.lock() = Some(handle);
}
