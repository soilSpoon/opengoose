/// Defines a database-serializable enum with `as_str()` and `parse()` methods.
///
/// # Example
///
/// ```ignore
/// db_enum! {
///     /// Status of a run.
///     pub enum RunStatus {
///         Running => "running",
///         Completed => "completed",
///         Failed => "failed",
///     }
/// }
/// ```
///
/// Expands to a `#[derive(Debug, Clone, PartialEq, Eq)]` enum with:
/// - `pub fn as_str(&self) -> &'static str`
/// - `pub fn parse(s: &str) -> Result<Self, PersistenceError>`
macro_rules! db_enum {
    (
        $( #[$meta:meta] )*
        $vis:vis enum $Name:ident {
            $(
                $( #[$variant_meta:meta] )*
                $Variant:ident => $str:literal
            ),+ $(,)?
        }
    ) => {
        $( #[$meta] )*
        #[derive(Debug, Clone, PartialEq, Eq)]
        $vis enum $Name {
            $(
                $( #[$variant_meta] )*
                $Variant,
            )+
        }

        impl $Name {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$Variant => $str, )+
                }
            }

            pub fn parse(s: &str) -> Result<Self, $crate::error::PersistenceError> {
                match s {
                    $( $str => Ok(Self::$Variant), )+
                    other => Err($crate::error::PersistenceError::InvalidEnumValue(
                        format!("unknown {}: {other}", stringify!($Name))
                    )),
                }
            }
        }
    };
}

pub(crate) use db_enum;
