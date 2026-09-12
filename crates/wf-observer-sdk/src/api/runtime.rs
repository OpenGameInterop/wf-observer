//! Shared executor for SDK calls from Rust, native bindings, and the browser.

use super::ObserverError;

use std::future::Future;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(crate) async fn execute<F>(future: F) -> Result<F::Output, ObserverError>
where
    F: Future,
{
    Ok(future.await)
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub(crate) async fn execute<F>(future: F) -> Result<F::Output, ObserverError>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    use std::sync::OnceLock;

    use tokio::runtime::{Builder, Runtime};
    use tokio_util::task::AbortOnDropHandle;

    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

    let runtime = RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("failed to create the WF Observer async runtime: {error}"))
        })
        .as_ref()
        .map_err(|message| ObserverError::Runtime {
            message: message.clone(),
        })?;

    AbortOnDropHandle::new(runtime.spawn(future))
        .await
        .map_err(|error| ObserverError::Runtime {
            message: format!("WF Observer async task failed: {error}"),
        })
}

#[cfg(all(test, not(all(target_family = "wasm", target_os = "unknown"))))]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::mpsc,
        task::{Context, Poll, Waker},
        thread,
        time::Duration,
    };

    #[test]
    fn runs_futures_on_the_tokio_runtime() -> Result<(), super::ObserverError> {
        let caller = thread::current().id();
        let worker = pollster::block_on(super::execute(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            thread::current().id()
        }))?;

        assert_ne!(caller, worker);
        Ok(())
    }

    #[test]
    fn cancellation_drops_the_inner_future() {
        struct PendingUntilDropped(mpsc::Sender<()>);

        impl Future for PendingUntilDropped {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
                Poll::Pending
            }
        }

        impl Drop for PendingUntilDropped {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }

        let (sender, receiver) = mpsc::channel();
        let mut execution = Box::pin(super::execute(PendingUntilDropped(sender)));
        let mut context = Context::from_waker(Waker::noop());

        assert!(execution.as_mut().poll(&mut context).is_pending());
        drop(execution);
        assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());
    }
}
