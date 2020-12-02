// https://github.com/GWBasic/sync-tokens
// (c) Andrew Rondeau
// Apache 2.0 license
// See https://github.com/GWBasic/sync-tokens/blob/main/LICENSE

mod cancelation_token;
mod completion_token;

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex};

    use cooked_waker::{Wake, WakeRef, ViaRawPointer};

	#[derive(Debug, Clone)]
	pub struct TestWaker {
		shared_state: Arc<Mutex<TestWakerState>>
	}

	#[derive(Debug, Clone)]
	struct TestWakerState {
		woke: bool
	}

	impl TestWaker {
		pub fn new() -> TestWaker {
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
}