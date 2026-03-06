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

#[cfg(test)]
mod tests {
    db_enum! {
        /// Test enum for validating the macro.
        pub enum TestColor {
            Red => "red",
            Green => "green",
            Blue => "blue",
        }
    }

    #[test]
    fn test_as_str() {
        assert_eq!(TestColor::Red.as_str(), "red");
        assert_eq!(TestColor::Green.as_str(), "green");
        assert_eq!(TestColor::Blue.as_str(), "blue");
    }

    #[test]
    fn test_parse_valid() {
        assert_eq!(TestColor::parse("red").unwrap(), TestColor::Red);
        assert_eq!(TestColor::parse("green").unwrap(), TestColor::Green);
        assert_eq!(TestColor::parse("blue").unwrap(), TestColor::Blue);
    }

    #[test]
    fn test_parse_invalid() {
        let err = TestColor::parse("yellow").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("TestColor"), "error should mention type name");
        assert!(msg.contains("yellow"), "error should mention bad value");
    }

    #[test]
    fn test_roundtrip() {
        for color in [TestColor::Red, TestColor::Green, TestColor::Blue] {
            let s = color.as_str();
            assert_eq!(TestColor::parse(s).unwrap(), color);
        }
    }

    #[test]
    fn test_debug_and_clone() {
        let c = TestColor::Red;
        let c2 = c.clone();
        assert_eq!(c, c2);
        let debug = format!("{:?}", c);
        assert_eq!(debug, "Red");
    }
}
