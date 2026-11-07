use std::thread;
use std::thread::ThreadId;

pub mod perf;

const VALUE_ALREADY_TAKEN: &str = "bug: guard payload already taken";

pub trait Handler
where
    Self: Sized,
{
    type Payload;

    fn event(&self, event: Event);

    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload;

    fn exit_scope(&self, payload: Self::Payload);
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub thread_id: ThreadId,
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug)]
pub enum EventKind {
    RegisterQueryOps,
    DeregisterQueryOps,
    StoreValue,
    FreeValues(usize),
    RegisterMemo,
    DeregisterMemos(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct ScopedEvent {
    pub thread_id: ThreadId,
    pub kind: ScopedEventKind,
}

#[derive(Clone, Copy, Debug)]
pub enum ScopedEventKind {
    Query,
    Eval,
}

#[derive(Debug)]
pub(crate) struct ScopeGuard<'handler, H>
where
    H: Handler,
{
    handler: &'handler H,
    payload: Option<H::Payload>,
}

impl Handler for () {
    type Payload = ();

    fn event(&self, _: Event) {}

    fn enter_scope(&self, _: ScopedEvent) -> Self::Payload {}

    fn exit_scope(&self, (): Self::Payload) {}
}

impl Event {
    pub(crate) fn new(kind: EventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl ScopedEvent {
    pub(crate) fn new(kind: ScopedEventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl<'handler, H> ScopeGuard<'handler, H>
where
    H: Handler,
{
    pub(crate) fn new(handler: &'handler H, event: ScopedEvent) -> Self {
        Self {
            handler,
            payload: Some(handler.enter_scope(event)),
        }
    }
}

impl<H> Drop for ScopeGuard<'_, H>
where
    H: Handler,
{
    fn drop(&mut self) {
        self.handler
            .exit_scope(self.payload.take().expect(VALUE_ALREADY_TAKEN));
    }
}
