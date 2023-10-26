use proc_macro_error::proc_macro_error;
use syn::ItemMod;

mod r#impl;
mod suite;

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

#[proc_macro_attribute]
#[proc_macro_error]
pub fn suite_cfg(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn before_all(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn before_each(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn after_all(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn after_each(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn cleanup(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}
