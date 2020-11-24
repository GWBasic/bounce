use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
pub struct CompletionToken {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenState {
	canceled: bool,
	waker: Option<Waker>
}

/// Future that allows gracefully shutting down the server
impl CompletionToken {
	pub fn new() -> CompletionToken {
		CompletionToken {
			shared_state: Arc::new(Mutex::new(CancelationTokenState {
				canceled: false,
				waker: None
			}))
		}
	}

	/// Call to shut down the server
	pub fn complete(&self) {
		let mut shared_state = self.shared_state.lock().unwrap();

		shared_state.canceled = true;
		if let Some(waker) = shared_state.waker.take() {
			waker.wake()
		}
	}
}

impl Future for CompletionToken {
	type Output = ();

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut shared_state = self.shared_state.lock().unwrap();

		if shared_state.canceled {
            Poll::Ready(())
		} else {
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
		}
	}
}

impl Clone for CompletionToken {
	fn clone(&self) -> Self {
		CompletionToken {
			shared_state: self.shared_state.clone()
		}
	}
}