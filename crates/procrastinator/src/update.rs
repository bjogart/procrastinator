use crate::ActiveQuery;
use crate::ActiveQueryStack;
use crate::Revision;
use crate::Snapshot;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::storage::Storage;

pub(crate) struct QueryUpdate<'snap, S, H>
where
    S: Storage,
{
    snapshot: &'snap Snapshot<S, H>,
    pub(crate) commit: Option<PendingCommit<S>>,
}

pub(crate) struct PendingCommit<S>
where
    S: Storage,
{
    current_rev: Revision,
    memo_id: MemoId<S>,
    pub(crate) new_value_id: Option<S::Id>,
}

impl<'snap, S, H> QueryUpdate<'snap, S, H>
where
    S: Storage,
{
    pub(crate) const fn new(snapshot: &'snap Snapshot<S, H>) -> Self {
        Self {
            snapshot,
            commit: None,
        }
    }
}

impl<S, H> Drop for QueryUpdate<'_, S, H>
where
    S: Storage,
{
    fn drop(&mut self) {
        let Self { snapshot, commit } = self;
        let active_query = {
            let mut active_queries = snapshot.active_queries.borrow_mut();
            let ActiveQueryStack(active_queries) = &mut *active_queries;
            active_queries.pop()
        };
        if let (Some(commit), Some(ActiveQuery { deps })) = (commit.take(), active_query) {
            commit.apply(&snapshot.runtime.memos, deps);
        }
    }
}

impl<S> PendingCommit<S>
where
    S: Storage,
{
    pub(crate) const fn new(current_rev: Revision, memo_id: MemoId<S>) -> Self {
        Self {
            current_rev,
            memo_id,
            new_value_id: None,
        }
    }

    fn apply(self, memos: &Memos<S>, deps: Vec<MemoId<S>>) {
        let Self {
            current_rev,
            memo_id,
            new_value_id,
        } = self;
        memos.commit_eval(memo_id, current_rev, deps, new_value_id);
    }
}
