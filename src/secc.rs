//! Async SECC ( Skip Enabled Concurrent Channel ) implementation based on [`flume`].
//!
//! This is the channel implementation used by actors to send and receive messages.

use std::collections::VecDeque;

/// Create an unbounded SECC channel
///
/// > **note:** The type `T` should be efficiently clonable as calls to [`SeccReceiver::peek`]
/// > must clone the value. Using an [`Arc`] is one way to do this.
pub fn secc_unbounded<T: Clone>() -> (SeccSender<T>, SeccReceiver<T>) {
    let (flume_sender, flume_receiver) = flume::unbounded();

    (
        SeccSender::new(flume_sender),
        SeccReceiver::new(flume_receiver),
    )
}

/// Create a bounded SECC channel
///
/// > **note:** The type `T` should be efficiently clonable as calls to [`SeccReceiver::peek`]
/// > must clone the value. Using an [`Arc`] is one way to do this.
pub fn secc_bounded<T: Clone>(capacity: usize) -> (SeccSender<T>, SeccReceiver<T>) {
    let (flume_sender, flume_receiver) = flume::bounded(capacity);

    (
        SeccSender::new(flume_sender),
        SeccReceiver::new(flume_receiver),
    )
}

/// A SECC sender, which is actually just a newtype over a `flume::Sender`.
///
/// Implemented as a newtype just in case we have to add more to it later, so that we can modify
/// its internals without breaking its usage.
#[derive(Clone)]
pub struct SeccSender<T>(flume::Sender<T>);

impl<T> SeccSender<T> {
    // Create a [`SeccSender`] from a `flume` Sender.
    fn new(sender: flume::Sender<T>) -> Self {
        SeccSender(sender)
    }

    /// See [`flume::Sender::send`].
    pub fn send(&self, msg: T) -> Result<(), flume::SendError<T>> {
        self.0.send(msg)
    }

    /// See [`flume::Sender::try_send`].
    pub fn try_send(&self, msg: T) -> Result<(), flume::TrySendError<T>> {
        self.0.try_send(msg)
    }
}

/// A receiver for a SECC channel. It is a wrapper around a flume reciever along with a skipped
/// messages queue that is used to store any messages that are skipped with the skip function.
pub struct SeccReceiver<T> {
    /// The underlying flume channel receiver
    receiver: flume::Receiver<T>,
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
    // Create a [`SeccReceiver`] from a `flume` Receiver.
    fn new(receiver: flume::Receiver<T>) -> Self {
        SeccReceiver {
            receiver,
            peeked_message: None,
            skipped: VecDeque::new(),
            is_resetting: false,
            reset_until: 0,
        }
    }

    /// Peek at the next message in the channel
    pub async fn peek(&mut self) -> Result<T, flume::RecvError> {
        // If we already have a peeked message, return it
        if let Some(msg) = &self.peeked_message {
            Ok(msg.clone())

        // If we are resetting, peek the message from the skipped queue
        } else if self.is_resetting {
            // Get the next message in the queue
            if let Some(msg) = self.skipped.get(0) {
                Ok(msg.clone())

            } else {
                unreachable!("If we are resetting there shoud always be a message in the \
                skipped queue.");
            }

        // If we don't already have a peeked message and we aren't resetting
        } else {
            // Get the next message in the channel
            let msg = self.receiver.recv_async().await?;

            // Clone it and put it in our peeked message slot
            self.peeked_message = Some(msg.clone());

            // Return the message
            Ok(msg)
        }
    }

    /// Receive the next message in the channel
    pub async fn recv(&mut self) -> Result<T, flume::RecvError> {
        // If we are currently resetting
        if self.is_resetting {
            // Pop the next message off of the skipped queue
            if let Some(msg) = self.skipped.pop_front() {
                // Decrement the reset until cursor to make sure it stays pointing at the same message
                self.reset_until -= 1;

                // If this was the last message we were supposed to reset until
                if self.reset_until == 0 {
                    // Go out of resetting mode
                    self.is_resetting = false;
                }

                Ok(msg)
            // If there is no message, go out of resetting mode and return the next message in the channel
            } else {
                self.is_resetting = false;
                self.receiver.recv_async().await
            }

        // If we have a peeked message, return that one
        } else if let Some(msg) = self.peeked_message.take() {
            Ok(msg)

        // Get the message from the channel
        } else {
            self.receiver.recv_async().await
        }
    }

    /// Skip the next message in the channel
    pub async fn skip(&mut self) -> Result<(), flume::RecvError> {
        // Get the message to skip
        let msg = 
            // If we have a peeked message skip that one
            if let Some(msg) = self.peeked_message.take() {
                msg
            
            // If we are resetting, skip the one off of the top of the skipped queue
            } else if self.is_resetting {
                if let Some(msg) = self.skipped.pop_front() {
                    msg
                } else {
                    unreachable!("If we are resetting there should be a message in the skipped \
                                  queue.");
                }

            // Otherwise, get the next message from the channel and skip it
            } else {
                self.receiver.recv_async().await?
            };

        // Add it to the skipped message queue
        self.skipped.push_back(msg);

        Ok(())
    }

    /// Causes `recv` to return previously skipped messages untill ther are none, where it starts
    /// collecting the messages from the channel again
    pub fn reset_skip(&mut self) {
        // Go into resetting mode
        self.is_resetting = true;
        // Reset until the end of the skipped message queue
        self.reset_until = self.skipped.len() - 1;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    enum Mode {
        Bounded(usize),
        Unbounded,
    }

    fn get_channel<T: Clone>(mode: Mode) -> (SeccSender<T>, SeccReceiver<T>) {
        match mode {
            Mode::Bounded(capacity) => secc_bounded(capacity),
            Mode::Unbounded => secc_unbounded(),
        }
    }

    fn basic(mode: Mode) {
        smol::run(async move {
            // Create a secc channel
            let (sender, mut receiver) = secc_bounded(100);

            // Send a message
            sender.send(0).unwrap();

            // Receive the message
            assert_eq!(receiver.recv().await.unwrap(), 0);

            // Send another message
            sender.send(1).unwrap();

            // Peek at the message
            assert_eq!(receiver.peek().await.unwrap(), 1);

            // Send another message
            sender.send(2).unwrap();

            // Peek at the message again ( it shouldn't change )
            assert_eq!(receiver.peek().await.unwrap(), 1);

            // Receive the next message ( it should be the peeked one )
            assert_eq!(receiver.recv().await.unwrap(), 1);

            // Receive the next message ( it should be the next one in line )
            assert_eq!(receiver.recv().await.unwrap(), 2);

            // Send 4 new messages
            sender.send(3).unwrap();
            sender.send(4).unwrap();
            sender.send(5).unwrap();
            sender.send(6).unwrap();

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
            sender.send(7).unwrap();
            sender.send(8).unwrap();
            sender.send(9).unwrap();
            sender.send(10).unwrap();
            sender.send(11).unwrap();

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
    fn bounded_basic() {
        basic(Mode::Bounded(100));
    }

    #[test]
    fn unbounded_basic() {
        basic(Mode::Unbounded);
    }
}
