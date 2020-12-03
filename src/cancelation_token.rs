// https://github.com/GWBasic/sync-tokens
// (c) Andrew Rondeau
// Apache 2.0 license
// See https://github.com/GWBasic/sync-tokens/blob/main/LICENSE

//! Contains structs to assist in canceling ongoing operations. See [`CancelationToken`](struct.CancelationToken.html) or [`sync-tokens`](../index.html) for an example.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::future::{Either, select};

/// Allows canceling an asynchronous operation. Whoever has a [`CancelationToken`](struct.CancelationToken.html) can cancel an
/// operation that uses a [`Cancelable`](struct.Cancelable.html)
/// 
/// See example at [`sync-tokens`](../index.html)
#[derive(Debug)]
pub struct CancelationToken {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

/// Assists in canceling an asynchronous operation. Typically, this struct is kept private and
/// used with either [`allow_cancel()`](struct.CancelationToken.html#method.allow_cancel) or [`Self::future()`](struct.CancelationToken.html#method.future). A [`CancelationToken`](struct.CancelationToken.html) is given to whoever can
/// cancel operations
/// 
/// See example at [`sync-tokens`](../index.html)
#[derive(Debug)]
pub struct Cancelable {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

/// Future for use with [`Cancelable`](struct.Cancelable.html)
#[derive(Debug)]
pub struct CancelationTokenFuture {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenState {
	canceled: bool,
	waker: Option<Waker>
}

impl CancelationToken {
	#[allow(dead_code)]
	/// Creates a new [`CancelationToken`](struct.CancelationToken.html) and [`Cancelable`](struct.Cancelable.html)
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

	/// Cancels the operation. This can be called multiple times safely
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
	/// Allows canceling the future. canceled_result is what's returned when the [`CancelationToken`](struct.CancelationToken.html)
	/// is canceled. It is reccomended that the future return a [`Result`](https://doc.rust-lang.org/std/result/) so that canceled_result
	/// can be an error
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

	/// Returns a future that returns once the [`CancelationToken`](struct.CancelationToken.html) is canceled. Intended for use
	/// with select
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
    use async_std::prelude::*;
	use futures::future;
	use std::task::Context;

    use cooked_waker::IntoWaker;

	use super::*;
	use crate::tests::*;

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
    fn test_via_poll() {

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
	
	#[async_std::test]
	async fn test_via_allow_cancel() {

		let (cancelation_token, cancelable) = CancelationToken::new();
		let shared_state = cancelation_token.shared_state.clone();

		assert_not_canceled_no_waker(&shared_state);

		let result_future = future::ready("result");
		let result = cancelable.allow_cancel(result_future, "canceled").await;

		assert_eq!(result, "result", "Future canceled incorrectly");

		assert_not_canceled_no_waker(&shared_state);

		cancelation_token.cancel();

		assert_canceled(&shared_state);

		let pending_future = future::pending();
		let result = cancelable.allow_cancel(pending_future, "canceled").await;

		assert_eq!(result, "canceled", "Future not canceled");
	}

    #[async_std::test]
    async fn test_via_future() {

		let (cancelation_token, cancelable) = CancelationToken::new();
		let shared_state = cancelation_token.shared_state.clone();

		assert_not_canceled_no_waker(&shared_state);

		match select(cancelable.future(), future::ready(())).await {
			Either::Left(_) => panic!("Cancelation token isn't canceled"),
			Either::Right(_) => {}
		}

		cancelation_token.cancel();

		assert_canceled(&shared_state);

		match select(cancelable.future(), future::pending::<()>()).await {
			Either::Left(_) => {},
			Either::Right(_) => panic!("Cancelation didn't happen")
		}

		assert_canceled(&shared_state);
	}
}
