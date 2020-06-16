//! Async SECC ( Skip Enabled Concurrent Channel ) implementation based on [`async-channel`].
//!
//! This is the channel implementation used by actors to send and receive messages.

use std::collections::VecDeque;

use tracing::trace;

/// Create an unbounded SECC channel
///
/// > **note:** The type `T` should be efficiently clonable as calls to [`SeccReceiver::peek`]
/// > must clone the value. Using an [`Arc`] is one way to do this.
#[tracing::instrument]
pub fn secc_unbounded<T: Clone>() -> (SeccSender<T>, SeccReceiver<T>) {
    let (raw_sender, raw_receiver) = async_channel::unbounded();

    (
        SeccSender::new(raw_sender),
        SeccReceiver::new(raw_receiver),
    )
}

/// Create a bounded SECC channel
///
/// > **note:** The type `T` should be efficiently clonable as calls to [`SeccReceiver::peek`]
/// > must clone the value. Using an [`Arc`] is one way to do this.
#[tracing::instrument]
pub fn secc_bounded<T: Clone>(capacity: usize) -> (SeccSender<T>, SeccReceiver<T>) {
    let (raw_sender, raw_receiver) = async_channel::bounded(capacity);

    (
        SeccSender::new(raw_sender),
        SeccReceiver::new(raw_receiver),
    )
}

/// A SECC sender, which is actually just a newtype over a `async_channel::Sender`.
///
/// Implemented as a newtype just in case we have to add more to it later, so that we can modify
/// its internals without breaking its usage.
#[derive(Clone)]
pub struct SeccSender<T>(async_channel::Sender<T>);

impl<T> SeccSender<T> {
    // Create a [`SeccSender`] from a `async_channel` Sender.
    fn new(sender: async_channel::Sender<T>) -> Self {
        SeccSender(sender)
    }

    /// See [`async_channel::Sender::send`].
    pub async fn send(&self, msg: T) -> Result<(), async_channel::SendError<T>> {
        self.0.send(msg).await
    }

    /// See [`async_channel::Sender::try_send`].
    pub fn try_send(&self, msg: T) -> Result<(), async_channel::TrySendError<T>> {
        self.0.try_send(msg)
    }
}

/// A receiver for a SECC channel. It is a wrapper around a async_channel reciever along with a skipped
/// messages queue that is used to store any messages that are skipped with the skip function.
pub struct SeccReceiver<T> {
    /// The underlying async_channel channel receiver
    receiver: async_channel::Receiver<T>,
    /// A message that has been received and peeked with `peek()`
    peeked_message: Option<T>,
    /// The queue of messages that have been skipped by the receiver
    skipped: VecDeque<T>,
    /// The index in the skipped deque at which to stop resetting
    reset_until: usize,
    /// Whether or not we are currently in the process of resetting a skip
    is_resetting: bool,
}

impl<T: Clone> SeccReceiver<T> {
    // Create a [`SeccReceiver`] from a `async_channel` Receiver.
    fn new(receiver: async_channel::Receiver<T>) -> Self {
        SeccReceiver {
            receiver,
            peeked_message: None,
            skipped: VecDeque::new(),
            is_resetting: false,
            reset_until: 0,
        }
    }

    /// Peek at the next message in the channel
    #[tracing::instrument(skip(self))]
    pub async fn peek(&mut self) -> Result<T, async_channel::RecvError> {
        // If we already have a peeked message, return it
        if let Some(msg) = &self.peeked_message {
            trace!("Returning the value we have previously peeked at");
            Ok(msg.clone())

        // If we are resetting, peek the message from the skipped queue
        } else if self.is_resetting {
            trace!("We are in the middle of resetting");

            // Get the next message in the queue
            if let Some(msg) = self.skipped.get(0) {
                trace!("Grabbing the message off the top of the skipped queue");

                self.reset_until -= 1;
                trace!(self.reset_until, "Decremented self.reset_until");

                Ok(msg.clone())

            } else {
                unreachable!("If we are resetting there shoud always be a message in the \
                skipped queue.");
            }

        // If we don't already have a peeked message and we aren't resetting
        } else {
            // Get the next message in the channel
            trace!("Grabbing the next element in the channel");
            let msg = self.receiver.recv().await?;

            // Clone it and put it in our peeked message slot
            trace!("Sticking message in our peeked slot");
            self.peeked_message = Some(msg.clone());

            // Return the message
            Ok(msg)
        }
    }

    /// Receive the next message in the channel
    #[tracing::instrument(skip(self))]
    pub async fn recv(&mut self) -> Result<T, async_channel::RecvError> {
        // If we are currently resetting
        if self.is_resetting {
            trace!("We are in the middle of resetting");
            
            // Pop the next message off of the skipped queue
            trace!("Grabbing next element off of the skipped queue");                    
            if let Some(msg) = self.skipped.pop_front() {
                self.reset_until -= 1;
                trace!(self.reset_until, "Decremented reset_until");
                
                if self.reset_until == 0 {
                    trace!("reset_unitl == 0: going out of reset mode");
                    self.is_resetting = false;
                }

                trace!("Returning skipped message");
                Ok(msg)

            } else {
                unreachable!("There should be an element in the skipped queue as we are in \
                the middle of resetting still")
            }

        // If we have a peeked message, return that one
        } else if let Some(msg) = self.peeked_message.take() {
            trace!("Returning the message out of the peeked slot");
            Ok(msg)

        // Get the message from the channel
        } else {
            trace!("Getting next message from channel");
            self.receiver.recv().await
        }
    }

    /// Skip the next message in the channel
    #[tracing::instrument(skip(self))]
    pub async fn skip(&mut self) -> Result<(), async_channel::RecvError> {
        // Get the message to skip
        let msg = 
            // If we have a peeked message skip that one
            if let Some(msg) = self.peeked_message.take() {
                trace!("Selecting the message that is in the peeked slot");
                msg
            
            // If we are resetting, skip the one off of the top of the skipped queue
            } else if self.is_resetting {
                trace!("We are in the middle of resetting");

                if let Some(msg) = self.skipped.pop_front() {
                    trace!("Selecting the next message in the skipped queue");
                    msg
                } else {
                    unreachable!("If we are resetting there should be a message in the skipped \
                                  queue.");
                }

            // Otherwise, get the next message from the channel and skip it
            } else {
                trace!("Selecting the next message from the channel");
                self.receiver.recv().await?
            };

        // Add it to the skipped message queue
        trace!("Skipping selected message");
        self.skipped.push_back(msg);

        Ok(())
    }

    /// Causes `recv` to return previously skipped messages untill ther are none, where it starts
    /// collecting the messages from the channel again
    #[tracing::instrument(skip(self))]
    pub fn reset_skip(&mut self) {
        // Go into resetting mode
        self.is_resetting = true;
        // Reset until the end of the skipped message queue
        self.reset_until = self.skipped.len();
        trace!(self.reset_until, "Resetting skips. Going into reset mode.")
    }

    /// Gets the next receivable message and discards it, returning an error if the channel is empty
    #[tracing::instrument(skip(self))]
    pub async fn pop(&mut self) -> Result<(), async_channel::RecvError> {
        self.recv().await.map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug)]
    enum Mode {
        Bounded(usize),
        Unbounded,
    }

    fn init_logging() {
        tracing_subscriber::fmt::try_init().ok();
    }

    #[tracing::instrument]
    fn basic(mode: Mode) {
        smol::run(async move {
            // Create a secc channel
            let (sender, mut receiver) = secc_bounded(100);

            // Send a message
            sender.send(0).await.unwrap();

            // Receive the message
            assert_eq!(receiver.recv().await.unwrap(), 0);

            // Send another message
            sender.send(1).await.unwrap();

            // Peek at the message
            assert_eq!(receiver.peek().await.unwrap(), 1);

            // Send another message
            sender.send(2).await.unwrap();

            // Peek at the message again ( it shouldn't change )
            assert_eq!(receiver.peek().await.unwrap(), 1);

            // Receive the next message ( it should be the peeked one )
            assert_eq!(receiver.recv().await.unwrap(), 1);

            // Receive the next message ( it should be the next one in line )
            assert_eq!(receiver.recv().await.unwrap(), 2);

            // Send 4 new messages
            sender.send(3).await.unwrap();
            sender.send(4).await.unwrap();
            sender.send(5).await.unwrap();
            sender.send(6).await.unwrap();

            // Peek at the next message
            assert_eq!(receiver.peek().await.unwrap(), 3);

            // Skip the message ( should skip 3 )
            receiver.skip().await.unwrap();

            // Skip the next message as well ( should skip 4 )
            receiver.skip().await.unwrap();

            // Peek the next message ( should be 5 )
            assert_eq!(receiver.peek().await.unwrap(), 5);

            // Receive the next message ( should be 5 )
            assert_eq!(receiver.recv().await.unwrap(), 5);

            // Reset the skip
            receiver.reset_skip();

            // Receive the next message ( should be previously skipped 3 )
            assert_eq!(receiver.recv().await.unwrap(), 3);

            // Receive the next message ( should be previously skipped 4 )
            assert_eq!(receiver.recv().await.unwrap(), 4);

            // Receive the next message ( should be 6, the next one after previously skipped ones )
            assert_eq!(receiver.recv().await.unwrap(), 6);

            // Send 5 new messages
            sender.send(7).await.unwrap();
            sender.send(8).await.unwrap();
            sender.send(9).await.unwrap();
            sender.send(10).await.unwrap();
            sender.send(11).await.unwrap();

            // Skip the next two messages ( skips 7 and 8 )
            receiver.skip().await.unwrap();
            receiver.skip().await.unwrap();

            // Receive the next message ( should be 9 )
            assert_eq!(receiver.recv().await.unwrap(), 9);

            // Reset skip
            receiver.reset_skip();

            // Peek the next message ( should be the previously skipped message 7 )
            assert_eq!(receiver.peek().await.unwrap(), 7);

            // Skip this message ( 7 )
            receiver.skip().await.unwrap();

            // Receive the next message ( should be previously skipped message 8 )
            assert_eq!(receiver.recv().await.unwrap(), 8);

            // Receive the next message ( should be 10 as we've exhausted our skip queue )
            assert_eq!(receiver.recv().await.unwrap(), 10);

            // Reset skip
            receiver.reset_skip();

            // Receive the next message ( should be 7 which was skipped a second time earlier )
            assert_eq!(receiver.recv().await.unwrap(), 7);

            // Receive the next message ( should be 11 )
            assert_eq!(receiver.recv().await.unwrap(), 11);
        });
    }
    #[test]
    fn secc_bounded_basic() {
        init_logging();
        basic(Mode::Bounded(100));
    }

    #[test]
    fn secc_unbounded_basic() {
        init_logging();
        basic(Mode::Unbounded);
    }
}
