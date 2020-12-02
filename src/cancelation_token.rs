// https://github.com/GWBasic/sync-tokens
// (c) Andrew Rondeau
// Apache 2.0 license
// See https://github.com/GWBasic/sync-tokens/blob/main/LICENSE

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
	#[allow(dead_code)]
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
	#[allow(dead_code)]
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

	#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
	use std::task::Context;

	use cooked_waker::{Wake, WakeRef, IntoWaker, ViaRawPointer};

	use super::*;

	#[derive(Debug, Clone)]
	struct TestWaker {
		shared_state: Arc<Mutex<TestWakerState>>
	}

	#[derive(Debug, Clone)]
	struct TestWakerState {
		woke: bool
	}

	impl TestWaker {
		fn new() -> TestWaker {
			TestWaker {
				shared_state: Arc::new(Mutex::new(TestWakerState {
					woke: false
				}))
			}
		}
	}

	impl WakeRef for TestWaker {
		fn wake_by_ref(&self) {
			let mut shared_state = self.shared_state.lock().unwrap();
			shared_state.woke = true;
		}
	}

	impl Wake for TestWaker {
		fn wake(self) {
			self.wake_by_ref();
		}
	}

	impl ViaRawPointer for TestWaker {
		type Target = ();
	
		fn into_raw(self) -> *mut () {
			let shared_state_ptr = Arc::into_raw(self.shared_state);
			shared_state_ptr as *mut ()
		}
	
		unsafe fn from_raw(ptr: *mut ()) -> Self {
			TestWaker {
				shared_state: Arc::from_raw(ptr as *const std::sync::Mutex<TestWakerState>)
			}
		}
	}

	fn assert_not_canceled_no_waker(shared_state: &Arc<Mutex<CancelationTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.canceled, false, "Canceled should be false at construction");
		assert_eq!(shared_state.waker.is_none(), true, "Waker should not be set");
	}

	fn assert_not_canceled_waker_set(shared_state: &Arc<Mutex<CancelationTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.canceled, false, "Canceled should be false");
		assert_eq!(shared_state.waker.is_some(), true, "Waker should be set");
	}

	fn assert_canceled(shared_state: &Arc<Mutex<CancelationTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.canceled, true, "Canceled should be true");
		assert_eq!(shared_state.waker.is_none(), true, "Waker should be set");
	}

    #[test]
    fn test_via_future() {

		let (cancelation_token, cancelable) = CancelationToken::new();
		let mut future = cancelable.future();
		let pinned_future = Pin::new(&mut future);

		let shared_state = cancelation_token.shared_state.clone();

		assert_not_canceled_no_waker(&shared_state);

		let test_waker = TestWaker::new();
		let waker = test_waker.clone().into_waker();
		let mut cx = Context::from_waker(&waker);

		let poll_result = pinned_future.poll(&mut cx);
		assert_eq!(poll_result.is_pending(), true, "Cancelation token should be pending");

		assert_not_canceled_waker_set(&shared_state);

		cancelation_token.cancel();

		assert_canceled(&shared_state);

		let pinned_future = Pin::new(&mut future);

		let poll_result = pinned_future.poll(&mut cx);
		assert_eq!(poll_result.is_ready(), true, "Cancelation token should be ready");

		assert_canceled(&shared_state);
    }
}
