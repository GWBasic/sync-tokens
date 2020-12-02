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
	canceled: bool,
	waker: Option<Waker>
}

// Todo: Split into Completable

/// Future that allows gracefully shutting down the server
impl CompletionToken {
	pub fn new() -> (CompletionToken, Completable) {
		let shared_state = Arc::new(Mutex::new(CompletionTokenState {
			canceled: false,
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