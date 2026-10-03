//! One-way records, declared beside native checked values with the same field source.

/// Export facts without restoring a session, watch, intent or executable plan.
pub trait ToDiagnostic {
    /// Owned data suitable for read-only history. It is not a native API input.
    type Record;
    /// Copy the current value into its diagnostic representation, without I/O.
    fn diagnostic(&self) -> Self::Record;
}

macro_rules! identity {
    ($($ty:ty),* $(,)?) => {$(
        impl ToDiagnostic for $ty {
            type Record = Self;
            fn diagnostic(&self) -> Self { self.to_owned() }
        }
    )*};
}
identity!(
    bool,
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    f32,
    f64,
    String,
    crate::NativeBlockState
);

impl<T: ToDiagnostic> ToDiagnostic for Option<T> {
    type Record = Option<T::Record>;
    fn diagnostic(&self) -> Self::Record {
        self.as_ref().map(ToDiagnostic::diagnostic)
    }
}
impl<T: ToDiagnostic> ToDiagnostic for Vec<T> {
    type Record = Vec<T::Record>;
    fn diagnostic(&self) -> Self::Record {
        self.iter().map(ToDiagnostic::diagnostic).collect()
    }
}
impl<T: ToDiagnostic> ToDiagnostic for Box<T> {
    type Record = Box<T::Record>;
    fn diagnostic(&self) -> Self::Record {
        Box::new(self.as_ref().diagnostic())
    }
}
impl<T: ToDiagnostic, const N: usize> ToDiagnostic for [T; N] {
    type Record = [T::Record; N];
    fn diagnostic(&self) -> Self::Record {
        std::array::from_fn(|i| self[i].diagnostic())
    }
}
impl<K: Clone + Ord, V: ToDiagnostic> ToDiagnostic for std::collections::BTreeMap<K, V> {
    type Record = std::collections::BTreeMap<K, V::Record>;
    fn diagnostic(&self) -> Self::Record {
        self.iter()
            .map(|(k, v)| (k.clone(), v.diagnostic()))
            .collect()
    }
}
pub(crate) use identity;

// Checked values retain their original fields, visibility, derives and methods.
// A separate Deserialize record is generated from those fields. Native-only
// guards are never projected. Field types resolve through one-way projections.
macro_rules! diagnostic_record {
    (
        $(#[$meta:meta])* pub struct $native:ident => $record:ident {
            $($(#[$fm:meta])* $vis:vis $field:ident: $ty:ty),* $(,)?
        }
        $(native_only { $($(#[$hm:meta])* $hvis:vis $hidden:ident: $hty:ty),* $(,)? })?
        diagnostic_serde { $(#[$rm:meta])* }
    ) => {
        $(#[$meta])* pub struct $native {
            $($(#[$fm])* $vis $field: $ty,)*
            $($($(#[$hm])* $hvis $hidden: $hty,)*)?
        }
        #[doc = concat!("Diagnostic facts of `", stringify!($native), "`; cannot restore its native value.")]
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        $(#[$rm])* #[serde(deny_unknown_fields)]
        pub struct $record {
            $($(#[$fm])* #[doc = concat!("Recorded `", stringify!($field), "`.")]
                pub $field: <$ty as crate::diagnostic_projection::ToDiagnostic>::Record,)*
        }
        impl crate::diagnostic_projection::ToDiagnostic for $native {
            type Record = $record;
            fn diagnostic(&self) -> $record {
                $record { $($field: crate::diagnostic_projection::ToDiagnostic::diagnostic(&self.$field),)* }
            }
        }
        impl From<&$native> for $record {
            fn from(value: &$native) -> Self {
                crate::diagnostic_projection::ToDiagnostic::diagnostic(value)
            }
        }
    };
    (
        $(#[$meta:meta])* pub enum $native:ident => $record:ident {
            $($(#[$vm:meta])* $variant:ident $({
                $($(#[$fm:meta])* $field:ident: $ty:ty),* $(,)?
            })?),* $(,)?
        }
        diagnostic_serde { $(#[$rm:meta])* }
    ) => {
        $(#[$meta])* pub enum $native {
            $($(#[$vm])* $variant $({ $($(#[$fm])* $field: $ty,)* })?,)*
        }
        #[doc = concat!("Diagnostic facts of `", stringify!($native), "`; cannot restore its native value.")]
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        $(#[$rm])* #[serde(deny_unknown_fields)]
        pub enum $record {
            $($(#[$vm])* $variant $({
                $($(#[$fm])* $field: <$ty as crate::diagnostic_projection::ToDiagnostic>::Record,)*
            })?,)*
        }
        impl crate::diagnostic_projection::ToDiagnostic for $native {
            type Record = $record;
            fn diagnostic(&self) -> $record {
                match self {
                    $(Self::$variant $({ $($field,)* })? => $record::$variant $({
                        $($field: crate::diagnostic_projection::ToDiagnostic::diagnostic($field),)*
                    })?,)*
                }
            }
        }
        impl From<&$native> for $record {
            fn from(value: &$native) -> Self {
                crate::diagnostic_projection::ToDiagnostic::diagnostic(value)
            }
        }
    };
}
pub(crate) use diagnostic_record;
