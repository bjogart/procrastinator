use crate::INCONSISTENT_STATE;
use crate::Revision;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::query_ops::QueryId;
use crate::storage::Storage;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug)]
pub(crate) struct Memos<S>(RwLock<MemoMap<S>>)
where
    S: Storage;

#[must_use]
#[derive(Debug)]
pub(crate) struct MemosTransfer<'memos, S>
where
    S: Storage,
{
    curr: RwLockReadGuard<'memos, MemoMap<S>>,
    next: MemoMap<S>,
}

type MemoMap<S> = HashMap<MemoId<S>, RwLock<MemoEntry<S>>>;

pub(crate) struct MemoId<S>
where
    S: Storage,
{
    pub(crate) query_id: QueryId,
    pub(crate) args_id: S::Id,
}

#[derive(Debug)]
pub(crate) struct MemoEntry<S>
where
    S: Storage,
{
    deps: Vec<MemoId<S>>,
    last_verified: Revision,
    last_changed: Revision,
    value_id: Option<S::Id>,
}

impl<S> Memos<S>
where
    S: Storage,
{
    pub(crate) fn new_input<H>(
        &self,
        handler: &H,
        rev: Revision,
        query_id: QueryId,
        args_id: S::Id,
        value_id: S::Id,
    ) -> MemoId<S>
    where
        H: event::Handler,
    {
        let memo_id = MemoId { query_id, args_id };
        let mut entry = MemoEntry::new();
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;

        handler.event(Event::new(EventKind::RegisterMemo));
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .insert(memo_id, RwLock::new(entry));

        memo_id
    }

    pub(crate) fn memo_id<H>(&self, handler: &H, query_id: QueryId, args_id: S::Id) -> MemoId<S>
    where
        H: event::Handler,
    {
        let memo_id = MemoId { query_id, args_id };
        let did_insert = match self.0.write().expect(INCONSISTENT_STATE).entry(memo_id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(RwLock::new(MemoEntry::new()));
                true
            }
        };
        // Database insertion must be finished before the handler is notified as
        // the handler may panic and poison the memo map lock.
        if did_insert {
            handler.event(Event::new(EventKind::RegisterMemo));
        }
        memo_id
    }

    #[expect(clippy::unwrap_in_result, reason = "Panics indicate bugs.")]
    pub(crate) fn value_id(&self, id: MemoId<S>) -> Option<S::Id> {
        self.0
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&id)
            .expect(UNKNOWN_ID)
            .read()
            .expect(INCONSISTENT_STATE)
            .value_id
    }

    pub(crate) fn last_changed(&self, id: MemoId<S>) -> Revision {
        self.0
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&id)
            .expect(UNKNOWN_ID)
            .read()
            .expect(INCONSISTENT_STATE)
            .last_changed
    }

    pub(crate) fn verification(&self, id: MemoId<S>) -> (Revision, Vec<MemoId<S>>) {
        let memos = self.0.read().expect(INCONSISTENT_STATE);
        let memo = memos
            .get(&id)
            .expect(UNKNOWN_ID)
            .read()
            .expect(INCONSISTENT_STATE);
        (memo.last_verified, memo.deps.clone())
    }

    pub(crate) fn mark_verified(&self, id: MemoId<S>, rev: Revision) {
        let memos = self.0.read().expect(INCONSISTENT_STATE);
        let mut memo = memos
            .get(&id)
            .expect(UNKNOWN_ID)
            .write()
            .expect(INCONSISTENT_STATE);
        // As a write lock is held on the memo entry, code after this comment
        // _must not_ panic to prevent lock poison.
        memo.last_verified = rev;
    }

    pub(crate) fn commit_eval(
        &self,
        id: MemoId<S>,
        rev: Revision,
        deps: Vec<MemoId<S>>,
        new_value_id: Option<S::Id>,
    ) {
        let memos = self.0.read().expect(INCONSISTENT_STATE);
        let mut memo = memos
            .get(&id)
            .expect(UNKNOWN_ID)
            .write()
            .expect(INCONSISTENT_STATE);
        // As a write lock is held on the memo entry, code after this comment
        // _must not_ panic to prevent lock poison.
        memo.last_verified = rev;
        memo.deps = deps;
        if let Some(value_id) = new_value_id {
            memo.value_id = Some(value_id);
            memo.last_changed = rev;
        }
    }

    pub(crate) fn set_input(&mut self, id: MemoId<S>, rev: Revision, value_id: S::Id) {
        let memos = self.0.get_mut().expect(INCONSISTENT_STATE);
        let memo = memos
            .get_mut(&id)
            .expect(UNKNOWN_ID)
            .get_mut()
            .expect(INCONSISTENT_STATE);
        memo.last_verified = rev;
        memo.last_changed = rev;
        memo.value_id = Some(value_id);
    }

    pub(crate) fn len(&self) -> usize {
        self.0.read().expect(INCONSISTENT_STATE).len()
    }

    pub(crate) fn transfer(&self, capacity: usize) -> MemosTransfer<'_, S> {
        MemosTransfer {
            curr: self.0.read().expect(INCONSISTENT_STATE),
            next: HashMap::with_capacity(capacity),
        }
    }
}

impl<S> MemosTransfer<'_, S>
where
    S: Storage,
{
    pub(crate) fn retain<F>(&mut self, memo_id: MemoId<S>, transfer: F) -> MemoId<S>
    where
        F: FnOnce(S::Id) -> (MemoId<S>, S::Id),
    {
        let (last_verified, last_changed, value_id) = {
            let MemoEntry {
                deps,
                last_verified,
                last_changed,
                value_id,
            } = &*self
                .curr
                .get(&memo_id)
                .expect(UNKNOWN_ID)
                .read()
                .expect(INCONSISTENT_STATE);
            assert!(
                deps.is_empty(),
                "bug: transferred input memo has dependencies"
            );
            let value_id = value_id.expect("bug: transferred input memo has no stored value");
            (*last_verified, *last_changed, value_id)
        };
        let (next_memo_id, next_value_id) = transfer(value_id);
        self.next.insert(
            next_memo_id,
            RwLock::new(MemoEntry {
                deps: Vec::new(),
                last_verified,
                last_changed,
                value_id: Some(next_value_id),
            }),
        );
        next_memo_id
    }

    pub(crate) fn into_memos(self) -> Memos<S> {
        let next = {
            let Self { next, curr: _curr } = self;
            next
        };
        Memos(RwLock::new(next))
    }
}

impl<S> Default for Memos<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(RwLock::default())
    }
}

impl<S> MemoEntry<S>
where
    S: Storage,
{
    const fn new() -> Self {
        Self {
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value_id: None,
        }
    }
}

impl<S> Clone for MemoId<S>
where
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for MemoId<S> where S: Storage {}

impl<S> PartialEq for MemoId<S>
where
    S: Storage,
{
    fn eq(&self, other: &Self) -> bool {
        self.query_id == other.query_id && self.args_id == other.args_id
    }
}

impl<S> Eq for MemoId<S> where S: Storage {}

impl<S> Hash for MemoId<S>
where
    S: Storage,
{
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.query_id.hash(state);
        self.args_id.hash(state);
    }
}

impl<S> Debug for MemoId<S>
where
    S: Storage,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self { query_id, args_id } = self;
        f.debug_struct("MemoId")
            .field("query_id", query_id)
            .field("args_id", args_id)
            .finish()
    }
}
