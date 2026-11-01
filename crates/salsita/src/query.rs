use crate::Snapshot;
use crate::event;
use crate::memo::MemoId;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::marker::PhantomData;

pub trait Query: Send + Sync + 'static {
    type Args: Clone + Eq + Hash + Send + Sync + 'static;
    type Out: Clone + Eq + Hash + Send + Sync + 'static;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler;
}

pub trait Input: Send + Sync + 'static {
    type Value: Clone + Eq + Hash + Send + Sync;
}

#[derive(Debug)]
pub(crate) struct InputRegistry<S>(Vec<InputEntry<S>>)
where
    S: Storage;

#[derive(Debug)]
struct InputEntry<S>
where
    S: Storage,
{
    memo_id: MemoId<S>,
    transfer: TransferInput<S>,
}

#[derive(Debug)]
pub(crate) struct InputTransfer<'entry, S>(&'entry InputEntry<S>)
where
    S: Storage;

type TransferInput<S> = fn(
    transfer: &mut <S as Storage>::Transfer<'_>,
    curr_args_id: <S as Storage>::Id,
    curr_value_id: <S as Storage>::Id,
) -> (<S as Storage>::Id, <S as Storage>::Id);

pub struct InputId<I>(usize, PhantomData<I>);

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<S, H>(_: &Snapshot<S, H>, _: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<S> InputRegistry<S>
where
    S: Storage,
{
    pub(crate) fn new_input<I>(&mut self, f: impl FnOnce(InputId<I>) -> MemoId<S>) -> InputId<I>
    where
        I: Input,
    {
        let Self(inputs) = self;
        let input_id = InputId(inputs.len(), PhantomData);
        let memo_id = f(input_id);
        inputs.push(InputEntry {
            memo_id,
            transfer: transfer::<S, I>,
        });
        return input_id;

        fn transfer<S, I>(
            transfer: &mut <S as Storage>::Transfer<'_>,
            curr_args_id: <S as Storage>::Id,
            curr_value_id: <S as Storage>::Id,
        ) -> (<S as Storage>::Id, <S as Storage>::Id)
        where
            S: Storage,
            I: Input,
        {
            let next_args_id = transfer.transfer::<InputId<I>>(curr_args_id);
            let next_value_id = transfer.transfer::<<I as Query>::Out>(curr_value_id);
            (next_args_id, next_value_id)
        }
    }

    pub(crate) fn memo_id<I>(&self, input_id: InputId<I>) -> MemoId<S> {
        let InputId(idx, _) = input_id;
        self.0
            .get(idx)
            .expect("bug: unknown input ID (was this ID created by another database?)")
            .memo_id
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn transfer(
        &self,
        mut transfer: impl FnMut(InputTransfer<'_, S>) -> MemoId<S>,
    ) -> Self {
        let next = self
            .0
            .iter()
            .map(|entry| InputEntry {
                memo_id: transfer(InputTransfer(entry)),
                transfer: entry.transfer,
            })
            .collect();
        Self(next)
    }
}

impl<S> InputTransfer<'_, S>
where
    S: Storage,
{
    pub(crate) fn memo_id(&self) -> MemoId<S> {
        self.0.memo_id
    }

    pub(crate) fn transfer(
        &self,
        storage_transfer: &mut S::Transfer<'_>,
        value_id: S::Id,
    ) -> (MemoId<S>, S::Id) {
        let Self(InputEntry {
            memo_id: MemoId { query_id, args_id },
            transfer: transfer_input,
        }) = self;
        let (next_args_id, next_value_id) = transfer_input(storage_transfer, *args_id, value_id);
        let next_memo_id = MemoId {
            query_id: *query_id,
            args_id: next_args_id,
        };
        (next_memo_id, next_value_id)
    }
}

impl<S> Default for InputRegistry<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<I> Clone for InputId<I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for InputId<I> {}

impl<I> PartialEq for InputId<I> {
    fn eq(&self, other: &Self) -> bool {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id == other_id && self_marker == other_marker
    }
}

impl<I> Eq for InputId<I> {}

impl<I> Hash for InputId<I> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I> Debug for InputId<I> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self(memo_id, _marker) = self;
        f.debug_tuple("InputId").field(&memo_id).finish()
    }
}
