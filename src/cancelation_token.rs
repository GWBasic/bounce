// I can't get code that uses this to compile

/*
use std::future::Future;
use std::io::Error;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::future::{Either, select};

#[derive(Debug)]
pub struct CancelationToken {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenFuture {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenState {
	canceled: bool,
	waker: Option<Waker>,
	error: Option<Error>
}

/// Future that allows gracefully shutting down the server
impl CancelationToken {
	pub fn new(error: Error) -> CancelationToken {
		CancelationToken {
			shared_state: Arc::new(Mutex::new(CancelationTokenState {
				canceled: false,
				waker: None,
				error: Some(error)
			}))
		}
	}

	/// Call to shut down the server
	pub fn cancel(&self) {
		let mut shared_state = self.shared_state.lock().unwrap();

		shared_state.canceled = true;
		if let Some(waker) = shared_state.waker.take() {
			waker.wake()
		}
	}

	pub async fn allow_cancel<T, TFuture>(&self, future: TFuture) -> async_std::io::Result<T> where
	TFuture: Future<Output = async_std::io::Result<T>> + Unpin {
		{
			let shared_state = self.shared_state.lock().unwrap();
			if shared_state.canceled {
				panic!("Already canceled");
			}
		}

		let cancelation_token_future = CancelationTokenFuture {
			shared_state: self.shared_state.clone()
		};

		match select(future, cancelation_token_future).await {
			Either::Left((r, _)) => r,
			Either::Right(_) => {
				let mut shared_state = self.shared_state.lock().unwrap();
				Err(shared_state.error.take().unwrap())
			}
		}
	}
}

impl Future for CancelationTokenFuture {
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

impl Clone for CancelationToken {
	fn clone(&self) -> Self {
		CancelationToken {
			shared_state: self.shared_state.clone()
		}
	}
}
*/