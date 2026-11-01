use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::panic_expected_different_type;
use crate::query::Query;
use crate::storage::Handle as _;
use crate::storage::Storage;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::RwLock;

#[derive(Debug)]
pub(crate) struct QueryOpsRegistry<S, H>(RwLock<HashMap<QueryId, QueryOps<S, H>>>)
where
    S: Storage;

#[derive(Debug)]
pub(crate) struct QueryOps<S, H>
where
    S: Storage,
{
    pub(crate) eval: Eval<S, H>,
    pub(crate) store_output: StoreOut<S, H>,
}

type Eval<S, H> =
    fn(snapshot: &Snapshot<S, H>, args: <S as Storage>::Handle) -> Box<dyn Any + Send + Sync>;

type StoreOut<S, H> =
    fn(storage: &S, handler: &H, value: &(dyn Any + Send + Sync)) -> <S as Storage>::Id;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct QueryId(TypeId);

impl<S, H> QueryOpsRegistry<S, H>
where
    S: Storage,
{
    pub(crate) fn query_id<Q>(&self, handler: &H) -> QueryId
    where
        H: event::Handler,
        Q: Query,
    {
        let Self(registry) = self;
        let query_id = QueryId(TypeId::of::<Q>());
        let did_insert = match registry.write().expect(INCONSISTENT_STATE).entry(query_id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(QueryOps::new::<Q>());
                true
            }
        };
        // Database insertion must be finished before the handler is
        // notified as the handler may panic and poison the registry lock.
        if did_insert {
            handler.event(Event::new(EventKind::RegisterQueryOps));
        }
        query_id
    }

    pub(crate) fn get(&self, query_id: QueryId) -> Option<QueryOps<S, H>>
    where
        H: event::Handler,
    {
        let Self(registry) = self;
        registry
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&query_id)
            .copied()
    }

    pub(crate) fn retain(&mut self, handler: &H, retain: impl Fn(QueryId) -> bool)
    where
        H: event::Handler,
    {
        let Self(registry) = self;
        let registry = registry.get_mut().expect(INCONSISTENT_STATE);
        registry.retain(|query_id, _| {
            let kept = retain(*query_id);
            if !kept {
                handler.event(Event::new(EventKind::DeregisterQueryOps));
            }
            kept
        });
        registry.shrink_to_fit();
    }
}

impl<S, H> Default for QueryOpsRegistry<S, H>
where
    S: Storage,
{
    fn default() -> Self {
        Self(RwLock::default())
    }
}

impl<S, H> QueryOps<S, H>
where
    S: Storage,
{
    fn new<Q>() -> Self
    where
        H: event::Handler,
        Q: Query,
    {
        return Self {
            eval: eval::<S, H, Q>,
            store_output: store_output::<S, H, Q::Out>,
        };

        fn eval<S, H, Q>(snapshot: &Snapshot<S, H>, args: S::Handle) -> Box<dyn Any + Send + Sync>
        where
            S: Storage,
            H: event::Handler,
            Q: Query,
        {
            let args = args.downcast::<Q::Args>();
            let out = Q::eval(snapshot, &args);
            Box::new(out)
        }

        fn store_output<S, H, T>(storage: &S, handler: &H, out: &(dyn Any + Send + Sync)) -> S::Id
        where
            S: Storage,
            H: event::Handler,
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            storage.store(handler, out)
        }
    }
}

impl<S, H> Clone for QueryOps<S, H>
where
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, H> Copy for QueryOps<S, H> where S: Storage {}
