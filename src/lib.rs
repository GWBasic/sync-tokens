// https://github.com/GWBasic/sync-tokens
// (c) Andrew Rondeau
// Apache 2.0 license
// See https://github.com/GWBasic/sync-tokens/blob/main/LICENSE

//! sync-tokens provides ways to coordinate with running tasks. It
//! provides a way to cleanly cancel a running task, and a way for
//! a running task to communicate when it's ready
//! 
//! ```toml
//! # Example, use the version numbers you need
//! sync-tokens = "0.1.0"
//! async-std = { version = "1.7.0", features = ["attributes"] }
//!```
//! 
//! # Examples
//! 
//! Accepts incoming sockets on a background task. Communicates when
//! the listener is actively listening, and allows canceling the loop
//! for incoming sockets
//! 
//! [See on github](https://github.com/GWBasic/sync-tokens-example)
//! 
//! ```no_run
//! use std::io::{ Error, ErrorKind };
//!
//! use async_std::io::Result;
//! use async_std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, SocketAddr};
//! use async_std::task;
//! use async_std::task::JoinHandle;
//! 
//! use sync_tokens::cancelation_token::{ Cancelable, CancelationToken };
//! use sync_tokens::completion_token::{ Completable, CompletionToken };
//! 
//! // Starts running a server on a background task
//! pub fn run_server() -> (JoinHandle<Result<()>>, CompletionToken<Result<SocketAddr>>, CancelationToken) {
//!     // This CompletionToken allows the caller to wait until the server is actually listening
//!     // The caller gets completion_token, which it can await on
//!     // completable is used to signal to completion_token
//!     let (completion_token, completable) = CompletionToken::new();
//! 
//!     // This CancelationToken allows the caller to stop the server
//!     // The caller gets cancelation_token
//!     // cancelable is used to allow canceling a call to await
//!     let (cancelation_token, cancelable) = CancelationToken::new();
//! 
//!     // The server is started on a background task, and the future returned
//!     let server_future = task::spawn(run_server_int(completable, cancelable));
//! 
//!     (server_future, completion_token, cancelation_token)
//! }
//! 
//! async fn run_server_int(completable: Completable<Result<SocketAddr>>, cancelable: Cancelable) -> Result<()> {
//! 
//!     let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
//!     let listener = TcpListener::bind(socket_addr).await?;
//! 
//!     // Inform that the server is listening
//!     let local_addr = listener.local_addr();
//!     completable.complete(local_addr);
//! 
//!     // Create a future that waits for an incoming socket
//!     let mut incoming_future = task::spawn(accept(listener));
//!     
//!     loop {
//!         // Wait for either the incoming socket (via incoming_future) or for the CancelationToken
//!         // to be canceled.
//!         // When the CancelationToken is canceled, the error is returned
//!         let (listener, _) = cancelable.allow_cancel(
//!             incoming_future, 
//!             Err(Error::new(ErrorKind::Interrupted, "Server terminated")))
//!             .await?;
//! 
//!         incoming_future = task::spawn(accept(listener));
//!     }
//! }
//! 
//! async fn accept(listener: TcpListener) -> Result<(TcpListener, TcpStream)> {
//!     let (stream, _) = listener.accept().await?;
//!     Ok((listener, stream))
//! }
//! 
//! #[async_std::main]
//! async fn main() {
//!     let (server_future, completion_token, cancelation_token) = run_server();
//! 
//!     println!("Server is starting");
//! 
//!     // Wait for the server to start
//!     let local_addr = completion_token.await.unwrap();
//! 
//!     println!("Server is listening at {}", local_addr);
//!     println!("Push Return to stop the server");
//! 
//!     let _ = std::io::stdin().read_line(&mut String::new()).unwrap();
//! 
//!     // Stop the server
//!     cancelation_token.cancel();
//! 
//!     // Wait for the server to shut down
//!     let err = server_future.await.unwrap_err();
//! 
//!     println!("Server ended: {}", err);
//! }
//! ```

#![cfg_attr(feature = "docs", feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(missing_debug_implementations, rust_2018_idioms)]
#![doc(test(attr(deny(rust_2018_idioms, warnings))))]
#![doc(test(attr(allow(unused_extern_crates, unused_variables))))]

pub mod cancelation_token;
pub mod completion_token;

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