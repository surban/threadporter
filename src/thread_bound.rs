//! Bind a value to a thread.

use futures_core::Stream;
use futures_sink::Sink;
use std::{
    any::type_name,
    fmt,
    future::Future,
    mem::{needs_drop, ManuallyDrop},
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
    thread,
    thread::ThreadId,
};

/// Binds the value to the current thread.
pub fn thread_bound<T>(value: T) -> ThreadBound<T> {
    ThreadBound::new(value)
}

/// Allows access to a value only from the thread that created this,
/// but always implements [`Send`] and [`Sync`].
///
/// This can be useful for WebAssembly targets, since all JavaScript objects are
/// `!Send` + `!Sync` but may occur within a type that must be
/// [`Send`] + [`Sync`], for example because it is also used in native platforms.
///
/// The [debug representation](fmt::Debug) can be safely used from any thread.
///
/// ### Panics
/// Panics if the inner value is accessed in any way from another thread
/// (including dropping if it needs drop).
pub struct ThreadBound<T> {
    value: ManuallyDrop<T>,
    thread_id: ThreadId,
    taken: bool,
}

unsafe impl<T> Send for ThreadBound<T> {}
unsafe impl<T> Sync for ThreadBound<T> {}

impl<T> ThreadBound<T> {
    /// Binds the value to the current thread.
    pub fn new(value: T) -> Self {
        Self { thread_id: thread::current().id(), value: ManuallyDrop::new(value), taken: false }
    }

    /// The id of the thread that is allowed to access the inner value.
    pub fn thread_id(this: &Self) -> ThreadId {
        this.thread_id
    }

    /// Takes the inner value out.
    ///
    /// ### Panics
    /// Panics if this was created by another thread.
    #[track_caller]
    pub fn into_inner(mut this: Self) -> T {
        this.check();
        this.taken = true;
        unsafe { ManuallyDrop::take(&mut this.value) }
    }

    /// Whether the value is usable from the current thread.
    #[inline]
    pub fn is_usable(this: &Self) -> bool {
        thread::current().id() == this.thread_id
    }

    #[inline]
    #[track_caller]
    fn check(&self) {
        if !Self::is_usable(self) {
            panic!(
                "cannot use {} on thread {:?} since it belongs to thread {:?}",
                type_name::<T>(),
                thread::current().id(),
                self.thread_id
            );
        }
    }
}

impl<T> Deref for ThreadBound<T> {
    type Target = T;
    #[track_caller]
    fn deref(&self) -> &T {
        self.check();
        &self.value
    }
}

impl<T> DerefMut for ThreadBound<T> {
    #[track_caller]
    fn deref_mut(&mut self) -> &mut T {
        self.check();
        &mut self.value
    }
}

impl<T> fmt::Debug for ThreadBound<T>
where
    T: fmt::Debug,
{
    #[track_caller]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut d = f.debug_struct("ThreadBound");
        d.field("thread_id", &self.thread_id);
        if Self::is_usable(self) {
            d.field("value", &self.value);
        }
        d.finish()
    }
}

impl<T> fmt::Display for ThreadBound<T>
where
    T: fmt::Display,
{
    #[track_caller]
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.check();
        self.value.fmt(f)
    }
}

impl<T> Default for ThreadBound<T>
where
    T: Default,
{
    #[track_caller]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Clone for ThreadBound<T>
where
    T: Clone,
{
    #[track_caller]
    fn clone(&self) -> Self {
        self.check();
        Self { thread_id: self.thread_id, value: self.value.clone(), taken: self.taken }
    }
}

impl<T> std::borrow::Borrow<T> for ThreadBound<T> {
    #[track_caller]
    fn borrow(&self) -> &T {
        self.check();
        &self.value
    }
}

impl<T> std::borrow::BorrowMut<T> for ThreadBound<T> {
    #[track_caller]
    fn borrow_mut(&mut self) -> &mut T {
        self.check();
        &mut self.value
    }
}

impl<T> PartialEq for ThreadBound<T>
where
    T: PartialEq,
{
    #[track_caller]
    fn eq(&self, other: &ThreadBound<T>) -> bool {
        self.check();
        other.check();
        self.value.eq(&other.value)
    }
}

impl<T> PartialEq<T> for ThreadBound<T>
where
    T: PartialEq,
{
    #[track_caller]
    fn eq(&self, other: &T) -> bool {
        self.check();
        (*self.value).eq(other)
    }
}

impl<T> Eq for ThreadBound<T> where T: Eq {}

impl<T> PartialOrd for ThreadBound<T>
where
    T: PartialOrd,
{
    #[track_caller]
    fn partial_cmp(&self, other: &ThreadBound<T>) -> Option<std::cmp::Ordering> {
        self.check();
        other.check();
        self.value.partial_cmp(&other.value)
    }
}

impl<T> PartialOrd<T> for ThreadBound<T>
where
    T: PartialOrd,
{
    #[track_caller]
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        self.check();
        (*self.value).partial_cmp(other)
    }
}

impl<T> Ord for ThreadBound<T>
where
    T: Ord,
{
    #[track_caller]
    fn cmp(&self, other: &ThreadBound<T>) -> std::cmp::Ordering {
        self.check();
        other.check();
        self.value.cmp(&other.value)
    }
}

impl<T> std::hash::Hash for ThreadBound<T>
where
    T: std::hash::Hash,
{
    #[track_caller]
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.check();
        self.value.hash(state)
    }
}

impl<T> Drop for ThreadBound<T> {
    #[track_caller]
    fn drop(&mut self) {
        if needs_drop::<T>() && !self.taken {
            self.check();
            unsafe { ManuallyDrop::drop(&mut self.value) };
        }
    }
}

impl<T> Future for ThreadBound<T>
where
    T: Future,
{
    type Output = T::Output;

    #[track_caller]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.check();
        let future = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        future.poll(cx)
    }
}

impl<T, S> Sink<S> for ThreadBound<T>
where
    T: Sink<S>,
{
    type Error = T::Error;

    #[track_caller]
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.check();
        let sink = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        sink.poll_ready(cx)
    }

    #[track_caller]
    fn start_send(self: Pin<&mut Self>, item: S) -> Result<(), Self::Error> {
        self.check();
        let sink = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        sink.start_send(item)
    }

    #[track_caller]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.check();
        let sink = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        sink.poll_flush(cx)
    }

    #[track_caller]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.check();
        let sink = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        sink.poll_close(cx)
    }
}

impl<T> Stream for ThreadBound<T>
where
    T: Stream,
{
    type Item = T::Item;

    #[track_caller]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.check();
        let stream = unsafe { self.map_unchecked_mut(|s| &mut *s.value) };
        stream.poll_next(cx)
    }
}
