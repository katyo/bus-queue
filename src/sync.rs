use super::*;
use std::iter::{IntoIterator, Iterator};
use std::thread;
use std::time::{Duration, Instant};

/// Provides an interface for the publisher
pub struct Publisher<T: Send> {
    bare_publisher: BarePublisher<T>,
    waker: Waker<thread::Thread>,
}

/// Provides an interface for subscribers
///
/// Every BusReader that can keep up with the push frequency should recv every pushed object.
/// BusReaders unable to keep up will miss object once the writer's index wi is larger then
/// reader's index ri + size
pub struct Subscriber<T: Send> {
    bare_subscriber: BareSubscriber<T>,
    sleeper: Sleeper<thread::Thread>,
}

pub fn channel<T: Send>(size: usize) -> (Publisher<T>, Subscriber<T>) {
    let (bare_publisher, bare_subscriber) = bare_channel(size);
    let (waker, sleeper) = alarm(thread::current());
    (
        Publisher {
            bare_publisher,
            waker,
        },
        Subscriber {
            bare_subscriber,
            sleeper,
        },
    )
}

impl<T: Send> Publisher<T> {
    /// Publishes values to the circular buffer at wi % size
    /// # Arguments
    /// * `object` - owned object to be published
    pub fn broadcast(&mut self, object: T) -> Result<(), SendError<T>> {
        self.bare_publisher.broadcast(object)?;
        self.waker.register_receivers();
        self.wake_all();
        Ok(())
    }
    pub fn wake_all(&self) {
        for sleeper in &self.waker.sleepers {
            sleeper.load().unpark();
        }
    }
}

impl<T: Send> Drop for Publisher<T> {
    fn drop(&mut self) {
        self.wake_all();
    }
}

impl<T: Send> Subscriber<T> {
    pub fn try_recv(&self) -> Result<Arc<T>, TryRecvError> {
        self.bare_subscriber.try_recv()
    }
    pub fn recv(&self) -> Result<Arc<T>, RecvError> {
        loop {
            let result = self.bare_subscriber.try_recv();
            if let Ok(object) = result {
                return Ok(object);
            }
            if let Err(e) = result {
                if let TryRecvError::Disconnected = e {
                    return Err(RecvError);
                }
            }
            self.sleeper.register(thread::current());
            thread::park();
        }
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Arc<T>, RecvTimeoutError> {
        loop {
            let result = self.bare_subscriber.try_recv();
            if let Ok(object) = result {
                return Ok(object);
            }
            if let Err(e) = result {
                if let TryRecvError::Disconnected = e {
                    return Err(RecvTimeoutError::Disconnected);
                }
            }
            self.sleeper.register(thread::current());
            let parking = Instant::now();
            thread::park_timeout(timeout);
            let unparked = Instant::now();
            if unparked.duration_since(parking) >= timeout {
                return Err(RecvTimeoutError::Timeout);
            }
        }
    }
    //    pub fn recv_deadline(&self, deadline: Instant) -> Result<T, RecvTimeoutError> {}
}

impl<T: Send> Clone for Subscriber<T> {
    fn clone(&self) -> Self {
        let arc_t = Arc::new(ArcSwap::new(Arc::new(thread::current())));
        self.sleeper.send(arc_t.clone());
        Self {
            bare_subscriber: self.bare_subscriber.clone(),
            sleeper: Sleeper {
                sender: self.sleeper.sender.clone(),
                sleeper: arc_t.clone(),
            },
        }
    }
}

//impl<'a, T> Iterator for &'a Subscriber<T> {}
//impl<T> Iterator for Subscriber<T>{}
//impl<'a, T> IntoIterator for &'a Subscriber<T> {}
//impl<T> IntoIterator for Subscriber<T>{}