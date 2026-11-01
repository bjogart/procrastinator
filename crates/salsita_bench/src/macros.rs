#![expect(
    non_snake_case,
    reason = "Inside macro declarations, the same identifier is used in type and variable scopes."
)]

use core::hash::Hash;
use core::marker::PhantomData;
use salsita::Snapshot;
use salsita::event;
use salsita::query::Input;
use salsita::query::Query;
use salsita::storage::Storage;

pub(crate) trait Op: Send + Sync + 'static {
    type Args: Clone + Eq + Hash + Send;
    type Out: Clone + Eq + Hash + Default + Send + Sync;
    fn op(args: Self::Args) -> Self::Out;
}

macro_rules! impl_dep {
    ($name:ident { input: $input:ident, deps: [$($dep:ident),*$(,)?]$(,)? }$(,)?) => {
        pub(crate) struct $name<O, $($dep,)*>(PhantomData<(O, $($dep),*)>);

        impl<O, $($dep,)*> Query for $name<O, $($dep,)*>
        where
            O: Op<Args = $input<$($dep::Out),*>>,
            $($dep: Query,)*
        {
            type Args = $input<$($dep::Args),*>;
            type Out = O::Out;

            fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
            where
                S: Storage,
                H: event::Handler,
            {
                let $input($($dep,)*) = args;
                $(let $dep = snapshot.query::<$dep>($dep);)*
                O::op($input($($dep.clone()),*))
            }
        }
    };
}

macro_rules! impl_add {
    ($name:ident { input: $input:ident, $($param:ident: $ty:ty),*$(,)? }) => {
        pub(crate) struct $name;
        impl Op for $name {
            type Args = $input<$($ty),*>;
            type Out = Option<usize>;

            fn op(args: Self::Args) -> Self::Out {
                let $input($($param),*) = args;
                Some(0 $(+ $param?)*)
            }
        }
    };
}

macro_rules! impl_inc {
    ($name:ident$(, $($($tt:tt)+)?)?) => {
        pub(crate) struct $name;
        impl Op for $name {
            type Args = Option<usize>;
            type Out = Option<usize>;

            fn op(args: Self::Args) -> Self::Out {
                Some(args? + 1)
            }
        }
        $($(impl_inc!($($tt)+);)?)?
    };
}

macro_rules! impl_tuple {
    ($name:ident($($param:ident),* $(,)?)) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) struct $name<$($param),*>($(pub(crate) $param),*);
    };
}

macro_rules! impl_inp {
    ($name:ident$(, $($($tt:tt)+)?)?) => {
        pub(crate) struct $name;

        impl Input for $name {
            type Value = Option<usize>;
        }
        $($(impl_inp!($($tt)+);)?)?
    };
}

pub(crate) struct Dep1<O, D>(PhantomData<(O, D)>);

impl<O, D> Query for Dep1<O, D>
where
    O: Op<Args = D::Out>,
    D: Query,
{
    type Args = D::Args;
    type Out = O::Out;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let d = snapshot.query::<D>(args).clone();
        O::op(d)
    }
}

impl_dep!(Dep3 {
    input: Tuple3,
    deps: [D1, D2, D3],
});

impl_dep!(Dep6 {
    input: Tuple6,
    deps: [D1, D2, D3, D4, D5, D6],
});

impl_dep!(Dep9 {
    input: Tuple9,
    deps: [D1, D2, D3, D4, D5, D6, D7, D8, D9],
});

impl_dep!(Dep10 {
    input: Tuple10,
    deps: [D1, D2, D3, D4, D5, D6, D7, D8, D9, D10],
});

impl_dep!(Dep27 {
    input: Tuple27,
    deps: [
        D1, D2, D3, D4, D5, D6, D7, D8, D9, D10, D11, D12, D13, D14, D15, D16, D17, D18, D19, D20,
        D21, D22, D23, D24, D25, D26, D27,
    ],
});

impl_dep!(Dep30 {
    input: Tuple30,
    deps: [
        D1, D2, D3, D4, D5, D6, D7, D8, D9, D10, D11, D12, D13, D14, D15, D16, D17, D18, D19, D20,
        D21, D22, D23, D24, D25, D26, D27, D28, D29, D30,
    ],
});

impl_dep!(Dep100 {
    input: Tuple100,
    deps: [
        D1, D2, D3, D4, D5, D6, D7, D8, D9, D10, D11, D12, D13, D14, D15, D16, D17, D18, D19, D20,
        D21, D22, D23, D24, D25, D26, D27, D28, D29, D30, D31, D32, D33, D34, D35, D36, D37, D38,
        D39, D40, D41, D42, D43, D44, D45, D46, D47, D48, D49, D50, D51, D52, D53, D54, D55, D56,
        D57, D58, D59, D60, D61, D62, D63, D64, D65, D66, D67, D68, D69, D70, D71, D72, D73, D74,
        D75, D76, D77, D78, D79, D80, D81, D82, D83, D84, D85, D86, D87, D88, D89, D90, D91, D92,
        D93, D94, D95, D96, D97, D98, D99, D100,
    ],
});

impl_add!(Add3 {
    input: Tuple3,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
});

impl_add!(Add6 {
    input: Tuple6,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
});

impl_add!(Add9 {
    input: Tuple9,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
    a7: Option<usize>,
    a8: Option<usize>,
    a9: Option<usize>,
});

impl_add!(Add10 {
    input: Tuple10,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
    a7: Option<usize>,
    a8: Option<usize>,
    a9: Option<usize>,
    a10: Option<usize>,
});

impl_add!(Add27 {
    input: Tuple27,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
    a7: Option<usize>,
    a8: Option<usize>,
    a9: Option<usize>,
    a10: Option<usize>,
    a11: Option<usize>,
    a12: Option<usize>,
    a13: Option<usize>,
    a14: Option<usize>,
    a15: Option<usize>,
    a16: Option<usize>,
    a17: Option<usize>,
    a18: Option<usize>,
    a19: Option<usize>,
    a20: Option<usize>,
    a21: Option<usize>,
    a22: Option<usize>,
    a23: Option<usize>,
    a24: Option<usize>,
    a25: Option<usize>,
    a26: Option<usize>,
    a27: Option<usize>,
});

impl_add!(Add30 {
    input: Tuple30,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
    a7: Option<usize>,
    a8: Option<usize>,
    a9: Option<usize>,
    a10: Option<usize>,
    a11: Option<usize>,
    a12: Option<usize>,
    a13: Option<usize>,
    a14: Option<usize>,
    a15: Option<usize>,
    a16: Option<usize>,
    a17: Option<usize>,
    a18: Option<usize>,
    a19: Option<usize>,
    a20: Option<usize>,
    a21: Option<usize>,
    a22: Option<usize>,
    a23: Option<usize>,
    a24: Option<usize>,
    a25: Option<usize>,
    a26: Option<usize>,
    a27: Option<usize>,
    a28: Option<usize>,
    a29: Option<usize>,
    a30: Option<usize>,
});

impl_add!(Add100 {
    input: Tuple100,
    a1: Option<usize>,
    a2: Option<usize>,
    a3: Option<usize>,
    a4: Option<usize>,
    a5: Option<usize>,
    a6: Option<usize>,
    a7: Option<usize>,
    a8: Option<usize>,
    a9: Option<usize>,
    a10: Option<usize>,
    a11: Option<usize>,
    a12: Option<usize>,
    a13: Option<usize>,
    a14: Option<usize>,
    a15: Option<usize>,
    a16: Option<usize>,
    a17: Option<usize>,
    a18: Option<usize>,
    a19: Option<usize>,
    a20: Option<usize>,
    a21: Option<usize>,
    a22: Option<usize>,
    a23: Option<usize>,
    a24: Option<usize>,
    a25: Option<usize>,
    a26: Option<usize>,
    a27: Option<usize>,
    a28: Option<usize>,
    a29: Option<usize>,
    a30: Option<usize>,
    a31: Option<usize>,
    a32: Option<usize>,
    a33: Option<usize>,
    a34: Option<usize>,
    a35: Option<usize>,
    a36: Option<usize>,
    a37: Option<usize>,
    a38: Option<usize>,
    a39: Option<usize>,
    a40: Option<usize>,
    a41: Option<usize>,
    a42: Option<usize>,
    a43: Option<usize>,
    a44: Option<usize>,
    a45: Option<usize>,
    a46: Option<usize>,
    a47: Option<usize>,
    a48: Option<usize>,
    a49: Option<usize>,
    a50: Option<usize>,
    a51: Option<usize>,
    a52: Option<usize>,
    a53: Option<usize>,
    a54: Option<usize>,
    a55: Option<usize>,
    a56: Option<usize>,
    a57: Option<usize>,
    a58: Option<usize>,
    a59: Option<usize>,
    a60: Option<usize>,
    a61: Option<usize>,
    a62: Option<usize>,
    a63: Option<usize>,
    a64: Option<usize>,
    a65: Option<usize>,
    a66: Option<usize>,
    a67: Option<usize>,
    a68: Option<usize>,
    a69: Option<usize>,
    a70: Option<usize>,
    a71: Option<usize>,
    a72: Option<usize>,
    a73: Option<usize>,
    a74: Option<usize>,
    a75: Option<usize>,
    a76: Option<usize>,
    a77: Option<usize>,
    a78: Option<usize>,
    a79: Option<usize>,
    a80: Option<usize>,
    a81: Option<usize>,
    a82: Option<usize>,
    a83: Option<usize>,
    a84: Option<usize>,
    a85: Option<usize>,
    a86: Option<usize>,
    a87: Option<usize>,
    a88: Option<usize>,
    a89: Option<usize>,
    a90: Option<usize>,
    a91: Option<usize>,
    a92: Option<usize>,
    a93: Option<usize>,
    a94: Option<usize>,
    a95: Option<usize>,
    a96: Option<usize>,
    a97: Option<usize>,
    a98: Option<usize>,
    a99: Option<usize>,
    a100: Option<usize>,
});

impl_inc![
    Inc1, Inc1V2, Inc1V3, Inc1V4, Inc1V5, Inc1V6, Inc1V7, Inc1V8, Inc1V9, Inc1V10, Inc1V11,
    Inc1V12, Inc1V13, Inc1V14, Inc1V15, Inc1V16, Inc1V17, Inc1V18, Inc1V19, Inc1V20, Inc1V21,
    Inc1V22, Inc1V23, Inc1V24, Inc1V25, Inc1V26, Inc1V27, Inc1V28, Inc1V29, Inc1V30, Inc1V31,
    Inc1V32, Inc1V33, Inc1V34, Inc1V35, Inc1V36, Inc1V37, Inc1V38, Inc1V39, Inc1V40, Inc1V41,
    Inc1V42, Inc1V43, Inc1V44, Inc1V45, Inc1V46, Inc1V47, Inc1V48, Inc1V49, Inc1V50, Inc1V51,
    Inc1V52, Inc1V53, Inc1V54, Inc1V55, Inc1V56, Inc1V57, Inc1V58, Inc1V59, Inc1V60, Inc1V61,
    Inc1V62, Inc1V63, Inc1V64, Inc1V65, Inc1V66, Inc1V67, Inc1V68, Inc1V69, Inc1V70, Inc1V71,
    Inc1V72, Inc1V73, Inc1V74, Inc1V75, Inc1V76, Inc1V77, Inc1V78, Inc1V79, Inc1V80, Inc1V81,
    Inc1V82, Inc1V83, Inc1V84, Inc1V85, Inc1V86, Inc1V87, Inc1V88, Inc1V89, Inc1V90, Inc1V91,
    Inc1V92, Inc1V93, Inc1V94, Inc1V95, Inc1V96, Inc1V97, Inc1V98, Inc1V99, Inc1V100,
];

impl_tuple!(Tuple3(T1, T2, T3));

impl_tuple!(Tuple6(T1, T2, T3, T4, T5, T6));

impl_tuple!(Tuple9(T1, T2, T3, T4, T5, T6, T7, T8, T9));

impl_tuple!(Tuple10(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10));

impl_tuple!(Tuple27(
    T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21,
    T22, T23, T24, T25, T26, T27,
));

impl_tuple!(Tuple30(
    T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21,
    T22, T23, T24, T25, T26, T27, T28, T29, T30,
));

impl_tuple!(Tuple100(
    T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21,
    T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40,
    T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59,
    T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78,
    T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97,
    T98, T99, T100
));

impl_inp![Inp, Inp2, Inp3, Inp4, Inp5, Inp6, Inp7, Inp8, Inp9];
