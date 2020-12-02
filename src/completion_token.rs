// https://github.com/GWBasic/sync-tokens
// (c) Andrew Rondeau
// Apache 2.0 license
// See https://github.com/GWBasic/sync-tokens/blob/main/LICENSE

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
pub struct CompletionToken {
	shared_state: Arc<Mutex<CompletionTokenState>>
}

#[derive(Debug)]
pub struct Completable {
	shared_state: Arc<Mutex<CompletionTokenState>>
}

#[derive(Debug)]
struct CompletionTokenState {
	complete: bool,
	waker: Option<Waker>
}

/// Future that allows gracefully shutting down the server
impl CompletionToken {
	#[allow(dead_code)]
	pub fn new() -> (CompletionToken, Completable) {
		let shared_state = Arc::new(Mutex::new(CompletionTokenState {
			complete: false,
			waker: None
		}));

		let completion_token = CompletionToken {
			shared_state: shared_state.clone()
		};

		let completable = Completable { shared_state };

		(completion_token, completable)
	}
}

impl Completable {
	/// Call to shut down the server
	#[allow(dead_code)]
	// TODO: Consider taking an argument to pass as the CompletionToken's output
	pub fn complete(&self) {
		let mut shared_state = self.shared_state.lock().unwrap();

		shared_state.complete = true;
		if let Some(waker) = shared_state.waker.take() {
			waker.wake()
		}
	}
}

impl Future for CompletionToken {
	type Output = (); // TODO: Make this configurable

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut shared_state = self.shared_state.lock().unwrap();

		if shared_state.complete {
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


#[cfg(test)]
mod tests {
    use async_std::prelude::*;
	use futures::future;
	use futures::future::{Either, select};
	use std::task::Context;

    use cooked_waker::IntoWaker;

	use super::*;
	use crate::tests::*;

	fn assert_not_completed_no_waker(shared_state: &Arc<Mutex<CompletionTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.complete, false, "Complete should be false at construction");
		assert_eq!(shared_state.waker.is_none(), true, "Waker should not be set");
	}

	fn assert_not_completed_waker_set(shared_state: &Arc<Mutex<CompletionTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.complete, false, "Complete should be false");
		assert_eq!(shared_state.waker.is_some(), true, "Waker should be set");
	}

	fn assert_completed(shared_state: &Arc<Mutex<CompletionTokenState>>) {
		let shared_state = shared_state.lock().unwrap();
		assert_eq!(shared_state.complete, true, "Complete should be true");
		assert_eq!(shared_state.waker.is_none(), true, "Waker should be set");
	}

    #[test]
    fn test_via_poll() {

		let (mut completion_token, completable) = CompletionToken::new();
		let shared_state = completion_token.shared_state.clone();

		let pinned_completion_token = Pin::new(&mut completion_token);

		assert_not_completed_no_waker(&shared_state);

		let test_waker = TestWaker::new();
		let waker = test_waker.clone().into_waker();
		let mut cx = Context::from_waker(&waker);

		let poll_result = pinned_completion_token.poll(&mut cx);
		assert_eq!(poll_result.is_pending(), true, "Completion token should be pending");

		assert_not_completed_waker_set(&shared_state);

		completable.complete();

		assert_completed(&shared_state);

		let pinned_completion_token = Pin::new(&mut completion_token);

		let poll_result = pinned_completion_token.poll(&mut cx);
		assert_eq!(poll_result.is_ready(), true, "Completion token should be ready");

		assert_completed(&shared_state);
	}

    #[async_std::test]
    async fn test_via_future() {

		let (mut completion_token, completable) = CompletionToken::new();
		let shared_state = completion_token.shared_state.clone();

		assert_not_completed_no_waker(&shared_state);

		match select(completion_token, future::ready(())).await {
			Either::Left(_) => panic!("Cancelation token isn't canceled"),
			Either::Right((_, c)) => completion_token = c
		}

		completable.complete();

		assert_completed(&shared_state);

		match select(completion_token, future::pending::<()>()).await {
			Either::Left(_) => {},
			Either::Right(_) => panic!("Cancelation didn't happen")
		}

		assert_completed(&shared_state);
	}
}
