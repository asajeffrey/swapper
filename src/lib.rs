use std::mem;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvError;
use std::sync::mpsc::Sender;
use std::sync::mpsc::SendError;

/// Concurrency control for atomic swap of ownership.
///
/// A common pattern for thread pools is that each thread owns a token,
/// and some times threads need to exchange tokens. A skeleton example
/// is:
///
/// ```rust
/// # use std::sync::mpsc::{Sender, Receiver};
/// struct Token;
/// enum Message {
///    // Messages go here
/// };
/// struct Thread {
///    sender_to_other_thread: Sender<Message>,
///    receiver_from_other_thread: Receiver<Message>,
///    token: Token,
/// }
/// impl Thread {
///    fn swap_token(&mut self) {
///       // This function should swap the token with the other thread.
///    }
///    fn handle(&mut self, message: Message) {
///        match message {
///           // Message handlers go here
///        }
///    }
///    fn run(&mut self) {
///       loop {
///          let message = self.receiver_from_other_thread.recv();
///          match message {
///             Ok(message) => self.handle(message),
///             Err(_) => return,
///          }
///       }
///    }
/// }
/// ```
///
/// To do this with the Rust channels, ownership of the token is first
/// passed from the thread to the channel, then to the other thead,
/// resulting in a transitory state where the thread does not have the
/// token. Typically to work round this, the thread stores an `Option<Token>`
/// rather than a `Token`:
///
/// ```rust
/// # use std::sync::mpsc::{self, Sender, Receiver};
/// # use std::mem;
/// # struct Token;
/// enum Message {
///    SwapToken(Token, Sender<Token>),
/// };
/// struct Thread {
///    sender_to_other_thread: Sender<Message>,
///    receiver_from_other_thread: Receiver<Message>,
///    token: Option<Token>, // ANNOYING Option
/// }
/// impl Thread {
///    fn swap_token(&mut self) {
///       let (sender, receiver) = mpsc::channel();
///       let token = self.token.take().unwrap();
///       self.sender_to_other_thread.send(Message::SwapToken(token, sender));
///       let token = receiver.recv().unwrap();
///       self.token = Some(token);
///    }
///    fn handle(&mut self, message: Message) {
///        match message {
///           Message::SwapToken(token, sender) => {
///              let token = mem::replace(&mut self.token, Some(token)).unwrap();
///              sender.send(token).unwrap();
///           }
///        }
///    }
/// }
/// ```
///
/// This crate provides a synchronization primitive for swapping ownership between threads.
/// The API is similar to channels, except that rather than separate `send(T)` and `recv():T`
/// methods, there is one `swap(T):T`, which swaps a `T` owned by one thread for a `T` owned
/// by the other. For example, it allows an implementation of the thread pool which always
/// owns a token.
///
/// ```rust
/// # use std::sync::mpsc::{self, Sender, Receiver};
/// # use swapper::{self, Swapper};
/// # struct Token;
/// enum Message {
///    SwapToken(Swapper<Token>),
/// };
/// struct Thread {
///    sender_to_other_thread: Sender<Message>,
///    receiver_from_other_thread: Receiver<Message>,
///    token: Token,
/// }
/// impl Thread {
///    fn swap_token(&mut self) {
///       let (our_swapper, their_swapper) = swapper::swapper();
///       self.sender_to_other_thread.send(Message::SwapToken(their_swapper));
///       our_swapper.swap(&mut self.token).unwrap();
///    }
///    fn handle(&mut self, message: Message) {
///        match message {
///           Message::SwapToken(swapper) => swapper.swap(&mut self.token).unwrap(),
///        }
///    }
/// }
/// ```

pub struct Swapper<T> {
    contents: Arc<AtomicPtr<T>>,
    wait: Receiver<()>,
    notify: Sender<()>,
}

impl<T> Swapper<T> {
    /// Swap data.
    ///
    /// If the other half of the swap pair is blocked waiting to swap, then it swaps ownership
    /// of the data, then unblocks the other thread. Otherwise it blocks waiting to swap.
    pub fn swap(&self, our_ref: &mut T) -> Result<(), SwapError> {
        loop {
            // Is the other thead blocked waiting to swap? If so, swap and unblock it.
            let their_ptr = self.contents.swap(ptr::null_mut(), Ordering::AcqRel);
            if let Some(their_ref) = unsafe { their_ptr.as_mut() } {
                // The safety of this implementation depends on the other thread being blocked
                // while this swap happens.
                mem::swap(our_ref, their_ref);
                // We have swapped ownership, so its now safe to unblock the other thread.
                try!(self.notify.send(()));
                return Ok(());
            }
            // Is the other thead not ready for a swap yet? If so, block waiting to swap.
            let their_ptr = self.contents.compare_and_swap(ptr::null_mut(), our_ref, Ordering::AcqRel);
            if their_ptr.is_null() {
                try!(self.wait.recv());
                return Ok(());
            }
        }
    }
}

/// Create a new pair of swappers.
pub fn swapper<T>() -> (Swapper<T>, Swapper<T>) {
    let contents = Arc::new(AtomicPtr::new(ptr::null_mut()));
    let (notify_a, wait_a) = mpsc::channel();
    let (notify_b, wait_b) = mpsc::channel();
    let swapper_a = Swapper {
        contents: contents.clone(),
        notify: notify_b,
        wait: wait_a,
    };
    let swapper_b = Swapper {
        contents: contents,
        notify: notify_a,
        wait: wait_b,
    };
    (swapper_a, swapper_b)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SwapError;

impl From<RecvError> for SwapError {
    fn from(_: RecvError) -> SwapError {
        SwapError
    }
}

impl From<SendError<()>> for SwapError {
    fn from(_: SendError<()>) -> SwapError {
        SwapError
    }
}
