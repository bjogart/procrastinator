use crate::handler::Handler;
use crate::panic_expected_different_type;
use alloc::sync::Arc;
use core::any::Any;
use core::fmt::Debug;
use core::hash::Hash;
use core::ops::Deref;

pub mod default;

pub trait Storage: Default + 'static {
    type Id: Clone + Copy + Eq + Hash + Debug + Send + Sync;
    type Handle: Debug + Handle;
    type Transfer<'storage>: Transfer<Storage = Self>
    where
        Self: 'storage;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn get(&self, id: Self::Id) -> Self::Handle;

    fn transfer(&self) -> Self::Transfer<'_>;
}

pub trait Transfer {
    type Storage: Storage;

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: Handler;
}

pub trait Handle {
    type TypedHandle<T>: Deref<Target = T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static;
}

impl Handle for Arc<dyn Any + Send + Sync + 'static> {
    type TypedHandle<T> = Arc<T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static,
    {
        Arc::downcast(self).unwrap_or_else(|_| panic_expected_different_type::<T>())
    }
}
