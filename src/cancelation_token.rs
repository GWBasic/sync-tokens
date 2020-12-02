use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::future::{Either, select};

#[derive(Debug)]
pub struct CancelationToken {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
pub struct Cancelable {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
pub struct CancelationTokenFuture {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenState {
	canceled: bool,
	waker: Option<Waker>
}

/// Future that allows gracefully shutting down the server
impl CancelationToken {
	pub fn new() -> (CancelationToken, Cancelable) {
		let shared_state = Arc::new(Mutex::new(CancelationTokenState {
			canceled: false,
			waker: None
		}));

		let cancelation_token = CancelationToken {
			shared_state: shared_state.clone()
		};
		
		let cancelable = Cancelable { shared_state };

		(cancelation_token, cancelable)
	}

	/// Call to shut down the server
	#[allow(dead_code)]
	pub fn cancel(&self) {
		let mut shared_state = self.shared_state.lock().unwrap();

		shared_state.canceled = true;
		if let Some(waker) = shared_state.waker.take() {
			waker.wake()
		}
	}
}

impl Cancelable {
	pub async fn allow_cancel<TFuture, T>(&self, future: TFuture, canceled_result: T) -> T where
	TFuture: Future<Output = T> + Unpin {
		{
			let shared_state = self.shared_state.lock().unwrap();
			if shared_state.canceled {
				return canceled_result;
			}
		}

		let cancelation_token_future = CancelationTokenFuture {
			shared_state: self.shared_state.clone()
		};

		match select(future, cancelation_token_future).await {
			Either::Left((l, _)) => l,
			Either::Right(_) => canceled_result
		}
	}

	pub fn future(&self) -> CancelationTokenFuture {
		CancelationTokenFuture {
			shared_state: self.shared_state.clone()
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

impl Clone for Cancelable {
	fn clone(&self) -> Self {
		Cancelable {
			shared_state: self.shared_state.clone()
		}
	}
}
