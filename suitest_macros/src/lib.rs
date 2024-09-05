use proc_macro_error::proc_macro_error;
use syn::ItemMod;

mod r#impl;
mod suite;

/// Annotate a test module to enable hook annotations.
/// Requires one parameter - the name of the test suite.
/// The whole test suite will contain only one function as far as cargo is concerned.
/// To run with output for individual tests, invoking `cargo test` with `--nocapture` is required.
///
/// ## Example
///
/// ```
/// #[suitest::suite(my_test_suite)]
/// mod my_tests {
///   use suitest::{before_all, after_all};
///   
///   // Set up global state.
///   #[before_all]
///   fn setup() -> String {
///     String::from("Hello world")
///   }
///
///   #[after_all]
///   fn teardown(hw: String) {
///     assert_eq!(hw, "Hello world");
///   }
///
///   #[test]
///   fn my_test() {
///     // ...
///   }
/// }
/// ```
#[proc_macro_attribute]
#[proc_macro_error]
pub fn suite(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let suite =
        syn::parse::<ItemMod>(input.clone()).expect("suitest can only be used on `mod` items");
    let id = syn::parse(attr).expect("invalid suite identifier");
    r#impl::impl_suite(id, suite).into()
}

/// Annotate the suite module to configure the suite.
///
/// `sequential = bool [false]` - Run the suite in sequence or in parallel.
///
/// `verbose = bool [false]` - Print what's going on when running the suite
#[proc_macro_attribute]
#[proc_macro_error]
pub fn suite_cfg(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// The annotated function runs when starting the test suite only once. Useful
/// for setting up the global state.
///
/// To insert items to the global state, add a single type or n-tuple return value
/// consisting of the types you wish to insert.
/// Then, add them as function arguments in any test/hook from which you wish to retrieve them.
///
/// ## Example
///
/// ```ignore
/// struct MyStruct {
///     a: bool,
/// }
///
/// #[suitest::before_all]
/// fn setup() -> (String, usize, MyStruct) {
///     (String::from("Hello world"), 420_usize, MyStruct { a: true })
/// }
///
/// #[test]
/// fn my_test(s: String, ms: MyStruct) {
///     assert_eq!(s, "Hello world");
///     assert!(ms.a);
/// }
/// ```
#[proc_macro_attribute]
#[proc_macro_error]
pub fn before_all(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// The annotated function runs before each test and can make local state available for it.
///
/// Local test states always have priority when retrieving items in tests.
///
/// The function can read the global state.
///
/// ## Example
///
/// ```ignore
/// use suitest::{before_all, before_each};
///
/// struct MyStruct {
///     a: bool,
/// }
///
/// #[before_all]
/// fn setup_global() -> MyStruct {
///     MyStruct { a: true }
/// }
///
/// #[before_each]
/// fn setup_local(my_struct: MyStruct) -> String {
///     assert!(ms.a);
///     String::from("Hello world")
/// }
///
/// #[test]
/// fn my_test(s: String, ms: MyStruct) {
///     assert_eq!(s, "Hello world");
///     assert!(ms.a);
/// }
/// ```
#[proc_macro_attribute]
#[proc_macro_error]
pub fn before_each(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// The annotated function runs only once when all the tests have passed.
///
/// Can only read the global state.
#[proc_macro_attribute]
#[proc_macro_error]
pub fn after_all(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// The annotated function runs after each passing test. Useful for cleaning up
/// state.
///
/// Can read the local state from `before_each` as well as the global state.
#[proc_macro_attribute]
#[proc_macro_error]
pub fn after_each(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// The annotated function runs whenever a test panics. Useful for having a
/// safety net to clear state if a test fails.
///
/// Can read from the local state from `before_each` as well as the global state.
#[proc_macro_attribute]
#[proc_macro_error]
pub fn cleanup(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}
