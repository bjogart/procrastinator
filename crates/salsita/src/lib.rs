extern crate alloc;

use crate::event::Event;
use crate::event::ScopedEvent;
use crate::event::ScopedEventKind;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::InputRegistry;
use crate::query::Query;
use crate::query_ops::QueryOpsRegistry;
use crate::shared::Shared;
use crate::storage::DefaultStorage;
use crate::storage::Handle;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use crate::update::PendingCommit;
use crate::update::QueryUpdate;
use core::any::type_name;
use core::cell::RefCell;
use core::mem;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use std::collections::HashSet;

pub mod event;
pub(crate) mod memo;
pub mod query;
mod query_ops;
mod shared;
pub mod storage;
#[cfg(test)]
mod tests;
mod update;

pub const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";

#[derive(Debug)]
pub struct Db<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    runtime: Shared<Runtime<S, H>>,
    inputs: InputRegistry<S>,
}

#[derive(Debug)]
pub struct Snapshot<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    runtime: Shared<Runtime<S, H>>,
    active_queries: RefCell<ActiveQueryStack<S>>,
}

#[derive(Debug)]
struct Runtime<S, H>
where
    S: Storage,
{
    rev: GlobalRevision,
    should_cancel: AtomicBool,
    storage: S,
    memos: Memos<S>,
    query_ops: QueryOpsRegistry<S, H>,
    handler: H,
}

#[derive(Debug)]
struct GlobalRevision {
    rev: Revision,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision {
    raw: usize,
}

#[derive(Debug)]
struct ActiveQueryStack<S>(Vec<ActiveQuery<S>>)
where
    S: Storage;

#[derive(Debug)]
struct ActiveQuery<S>
where
    S: Storage,
{
    deps: Vec<MemoId<S>>,
}

enum MemoValue<S>
where
    S: Storage,
{
    Memoized,
    Uncommitted(S::Id),
}

impl<S, H> Db<S, H>
where
    S: Storage,
{
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.runtime.handler
    }

    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I>
    where
        H: event::Handler,
        I: Input,
    {
        let rev = self.runtime.rev.get();
        let query_id = self.runtime.query_ops.query_id::<I>(&self.runtime.handler);
        let value_id = self.runtime.storage.store(&self.runtime.handler, value);
        self.inputs.new_input(|input_id| {
            let dummy_args_id = self.runtime.storage.store(&self.runtime.handler, &input_id);
            self.runtime.memos.new_input(
                &self.runtime.handler,
                rev,
                query_id,
                dummy_args_id,
                value_id,
            )
        })
    }

    pub fn set_input<I>(&mut self, input_id: InputId<I>, value: &I::Value)
    where
        H: event::Handler,
        I: Input,
    {
        self.runtime.should_cancel.store(true, Ordering::Relaxed);
        let runtime = self.runtime.wait_exclusive();
        *runtime.should_cancel.get_mut() = false;

        let memo_id = self.inputs.memo_id(input_id);
        let new_value_id = runtime.storage.store(&runtime.handler, value);
        if runtime.memos.value_id(memo_id) != Some(new_value_id) {
            let current_rev = runtime.rev.bump();
            runtime.memos.set_input(memo_id, current_rev, new_value_id);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot<S, H> {
        Snapshot::new(self.runtime.clone())
    }

    pub fn gc(&mut self)
    where
        H: event::Handler,
    {
        self.runtime.wait_exclusive().gc(&mut self.inputs);
    }
}

impl<S, H> Default for Db<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            runtime: Shared::default(),
            inputs: InputRegistry::default(),
        }
    }
}

impl<S, H> Snapshot<S, H>
where
    S: Storage,
{
    fn new(runtime: Shared<Runtime<S, H>>) -> Self {
        Self {
            runtime,
            active_queries: RefCell::default(),
        }
    }

    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.runtime.handler
    }

    pub fn query<Q>(&self, args: &Q::Args) -> <S::Handle as Handle>::TypedHandle<Q::Out>
    where
        H: event::Handler,
        Q: Query,
    {
        let _query_guard = &self
            .runtime
            .handler
            .scoped_event(ScopedEvent::new(ScopedEventKind::Query));
        let query_id = self.runtime.query_ops.query_id::<Q>(&self.runtime.handler);
        let args_id = self.runtime.storage.store(&self.runtime.handler, args);
        let memo_id = self
            .runtime
            .memos
            .memo_id(&self.runtime.handler, query_id, args_id);
        self.track_dep(memo_id);
        let value_id = match self.verify_memo(self.runtime.rev.get(), memo_id) {
            MemoValue::Memoized => self.memoized_value_id(memo_id),
            MemoValue::Uncommitted(value_id) => value_id,
        };
        Handle::downcast::<Q::Out>(self.runtime.storage.get(value_id))
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId<S>) -> MemoValue<S>
    where
        H: event::Handler,
    {
        let (last_verified, deps) = {
            let (last_verified, deps) = self.runtime.memos.verification(memo_id);
            if last_verified == current_rev
                || self.should_cancel() && last_verified > Revision::NEVER_VERIFIED
            {
                return MemoValue::Memoized;
            }
            (last_verified, deps)
        };
        let deps_postdate_memo = deps
            .into_iter()
            .any(|dep| self.dep_postdates_rev(current_rev, last_verified, dep));
        if !deps_postdate_memo && last_verified > Revision::NEVER_VERIFIED && !self.should_cancel()
        {
            self.runtime.memos.mark_verified(memo_id, current_rev);
            return MemoValue::Memoized;
        }
        self.eval_memo(current_rev, memo_id)
    }

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId<S>) -> MemoValue<S>
    where
        H: event::Handler,
    {
        let ops = self
            .runtime
            .query_ops
            .get(memo_id.query_id)
            .expect("bug: query not registered");
        let args = self.runtime.storage.get(memo_id.args_id);
        let mut query_update = self.install_query();
        let out = {
            let _eval_guard = &self
                .runtime
                .handler
                .scoped_event(ScopedEvent::new(ScopedEventKind::Eval));
            (ops.eval)(self, args)
        };
        let out = (ops.store_output)(&self.runtime.storage, &self.runtime.handler, out.as_ref());
        // Do not memoize the result if cancellation is active.
        if self.should_cancel() {
            return MemoValue::Uncommitted(out);
        }
        let mut commit = PendingCommit::new(current_rev, memo_id);
        if self.runtime.memos.value_id(memo_id) != Some(out) {
            commit.new_value_id = Some(out);
        }
        query_update.commit = Some(commit);
        MemoValue::Memoized
    }

    fn dep_postdates_rev(
        &self,
        current_rev: Revision,
        memo_last_verified: Revision,
        dep: MemoId<S>,
    ) -> bool
    where
        H: event::Handler,
    {
        self.verify_memo(current_rev, dep);
        self.runtime.memos.last_changed(dep) > memo_last_verified
    }

    fn install_query(&self) -> QueryUpdate<'_, S, H>
    where
        H: event::Handler,
    {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        active_queries.push(ActiveQuery::default());
        QueryUpdate::new(self)
    }

    fn track_dep(&self, dep: MemoId<S>) {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        let caller = active_queries.last_mut();
        if let Some(ActiveQuery { deps }) = caller {
            deps.push(dep);
        }
    }

    fn memoized_value_id(&self, memo_id: MemoId<S>) -> S::Id {
        self.runtime
            .memos
            .value_id(memo_id)
            .expect("bug: memo entry has no stored value (value not yet computed or memoized)")
    }

    #[must_use]
    pub(crate) fn should_cancel(&self) -> bool {
        // Cancellation does not end until all snapshots drop, so once a
        // snapshot sees `true` here, it will keep seeing true.
        self.runtime.should_cancel.load(Ordering::Relaxed)
    }
}

impl<S, H> Runtime<S, H>
where
    S: Storage,
{
    fn gc(&mut self, inputs: &mut InputRegistry<S>)
    where
        H: event::Handler,
    {
        let Self {
            storage,
            memos,
            query_ops,
            handler,
            rev: _,
            should_cancel: _,
        } = self;
        let input_count = inputs.len();
        let mut storage_transfer = storage.transfer();
        let mut memos_transfer = memos.transfer(input_count);
        let mut input_queries = HashSet::with_capacity(input_count);
        let next_inputs = inputs.transfer(|input| {
            let memo_id = input.memo_id();
            input_queries.insert(memo_id.query_id);
            memos_transfer.retain(memo_id, |value_id| {
                input.transfer(&mut storage_transfer, value_id)
            })
        });
        let next_memos = memos_transfer.into_memos();
        let next_storage = storage_transfer.into_storage(handler);

        let prev_memos = {
            // Replace everything first before dropping collected values, since
            // Drop implementations can panic.
            let _prev_inputs = mem::replace(inputs, next_inputs);
            let _prev_storage = mem::replace(storage, next_storage);
            mem::replace(memos, next_memos)
        };

        handler.event(Event::new(event::EventKind::DeregisterMemos(
            prev_memos.len().saturating_sub(memos.len()),
        )));
        query_ops.retain(handler, |query_id| input_queries.contains(&query_id));
    }
}

impl<S, H> Default for Runtime<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            rev: GlobalRevision::default(),
            should_cancel: AtomicBool::default(),
            storage: S::default(),
            memos: Memos::default(),
            query_ops: QueryOpsRegistry::default(),
            handler: H::default(),
        }
    }
}

impl<S> Default for ActiveQueryStack<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<S> Default for ActiveQuery<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self {
            deps: Vec::default(),
        }
    }
}

impl GlobalRevision {
    fn bump(&mut self) -> Revision {
        let Self {
            rev: Revision { raw },
        } = self;
        *raw = raw.strict_add(1);
        self.get()
    }

    fn get(&self) -> Revision {
        let Self { rev } = self;
        *rev
    }
}

impl Default for GlobalRevision {
    fn default() -> Self {
        Self {
            rev: Revision {
                raw: Revision::NEVER_VERIFIED.raw.strict_add(1),
            },
        }
    }
}

impl Revision {
    const NEVER_VERIFIED: Self = Self { raw: 0 };
}

pub(crate) fn panic_expected_different_type<T>() -> ! {
    panic!(
        "bug (type mismatch): expected `{}` but found different type (possible database mix-up)",
        type_name::<T>()
    )
}
