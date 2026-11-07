use crate::query::cancel::sealed::CancellationPolicy;
pub(crate) mod sealed {
    pub trait CancellationPolicy {
        const CAN_RETURN_STALE_RESULTS: bool;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fresh;

#[derive(Clone, Copy, Debug)]
pub struct Stale;

impl CancellationPolicy for Fresh {
    const CAN_RETURN_STALE_RESULTS: bool = false;
}

impl CancellationPolicy for Stale {
    const CAN_RETURN_STALE_RESULTS: bool = true;
}
