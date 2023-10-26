use proc_macro_error::abort;
use quote::{format_ident, quote};
use syn::{
    punctuated::Punctuated, spanned::Spanned, token::Comma, FnArg, Ident, ItemFn, ItemMod, Pat,
    Signature,
};

use crate::suite::{
    PathOrTupleExpr, PathOrTupleReturn, StateModifier, SuiteConfig, SuiteFn, TestFn, TestSuite,
};

pub fn impl_suite(id: Ident, item_mod: ItemMod) -> proc_macro2::TokenStream {
    // Skip empty modules
    if item_mod.content.is_none()
        || item_mod
            .content
            .as_ref()
            .is_some_and(|(_, items)| items.is_empty())
    {
        return quote!(#item_mod);
    }

    let (_, items) = item_mod.content.unwrap();

    let config = SuiteConfig::parse(&item_mod.attrs);

    let mut suite = TestSuite::new(id, config);

    // Holds all non-fn items
    let mut other = vec![];

    let mut i = 0;
    for item in items {
        // We are interested only in functions
        if let syn::Item::Fn(item) = item {
            suite.process_fn(&mut i, item);
        } else {
            other.push(item);
        }
    }

    let ItemMod {
        attrs,
        vis,
        mod_token,
        ident,
        ..
    } = item_mod;

    let TestSuite {
        suite_id,
        config,
        tests,
        before_all,
        before_each,
        after_all,
        after_each,
        cleanup,
    } = suite;

    // Used as identifiers for the tests and their hooks
    let ids = tests
        .iter()
        .map(|t| format_ident!("ID{}", t.id))
        .collect::<Vec<_>>();

    // Used as actual usize literals for the const generics
    let id_lits = tests.iter().map(|t| t.id).collect::<Vec<_>>();

    let len = tests.len();

    let verbose = config.verbose;

    let (before_all, ba_id) = quote_suite_fn(
        before_all.as_ref(),
        false,
        verbose.then_some("Running before_all"),
    );
    let (after_all, aa_id) = quote_suite_fn(
        after_all.as_ref(),
        false,
        verbose.then_some("Running after_all"),
    );
    let (before_each, be_id) = quote_suite_fn(
        before_each.as_ref(),
        true,
        verbose.then_some("Running before_each"),
    );
    let (after_each, ae_id) = quote_suite_fn(
        after_each.as_ref(),
        true,
        verbose.then_some("Running after_each"),
    );
    let (cleanup, cleanup_id) =
        quote_suite_fn(cleanup.as_ref(), true, verbose.then_some("Running cleanup"));

    let ba_invoke = ba_id.map(|id| quote!(#id().await;));

    let aa_invoke = aa_id.map(|id| quote!(#id().await;));

    let test_declarations = quote_test_declarations(&tests, verbose);

    let mut test_tasks = quote!();
    let mut tasks = vec![];

    for (test, const_id) in tests.iter().zip(ids.iter()) {
        let TestFn { id, item, .. } = test;

        let test_id = &item.sig.ident;
        let is_async = item.sig.asyncness.is_some();

        let be_invoke = be_id.as_ref().map(|id| quote!(#id::<#const_id>().await;));
        let ae_invoke = ae_id.as_ref().map(|id| quote!(#id::<#const_id>().await;));

        let task_id = format_ident!("test{id}");
        tasks.push(task_id.clone());

        let tokens = quote!(
            let #task_id = async move {
                #be_invoke
                #test_id::<#const_id>().await;
                #ae_invoke
            };
        );

        test_tasks.extend(tokens);
    }

    let exec = quote_exec(&tasks, &ids, &config, cleanup_id.as_ref());

    quote!(
        #(#attrs)*
        #vis #mod_token #ident {
            #(#other)*

            const GLOBAL: usize = usize::MAX;
            #(const #ids: usize = #id_lits;)*
            const IDS: [usize; #len] = [#(#ids),*];
            static STATE: ::suitest::internal::OnceCell<::suitest::State::<#verbose>> = ::suitest::internal::OnceCell::new();

            #before_all

            #before_each

            #after_each

            #after_all

            #test_declarations

            #cleanup

            #[tokio::test]
            async fn #suite_id () {
                let state = ::suitest::State::default();

                state.create_local_state(IDS).await;
                STATE.set(state).expect("state configuration failure");

                #test_tasks

                #ba_invoke

                #exec

                #aa_invoke
            }
        }
    )
}

fn quote_exec(
    tasks: &[Ident],
    ids: &[Ident],
    config: &SuiteConfig,
    cleanup: Option<&Ident>,
) -> proc_macro2::TokenStream {
    assert_eq!(tasks.len(), ids.len());

    match (config.sequential, cleanup) {
        (true, Some(cleanup)) => quote!(
            let mut errors: Vec<tokio::task::JoinError> = vec![];
            #(
                let res = tokio::spawn(#tasks).await;
                if let Err(e) = res {
                    #cleanup::<#ids>().await;
                    errors.push(res);
                }
            )*
            if let Some(e) = errors.pop() {
                std::panic::resume_unwind(e.into_panic())
            }
        ),
        (false, Some(cleanup)) => {
            quote!(
                let results = ::suitest::internal::futures_util::future::join_all(
                    vec![
                        #(tokio::spawn(#tasks)),*
                    ]
                ).await;
                let mut errors: Vec<tokio::task::JoinError> = vec![];
                for (i, result) in results.into_iter().enumerate() {
                    if let Err(e) = result {
                        match i {
                            #(#ids => {
                                #cleanup::<#ids>().await;
                                errors.push(e);
                            })*
                            _ => unreachable!(),
                        }
                    }
                }
                if let Some(e) = errors.pop() {
                    std::panic::resume_unwind(e.into_panic())
                }
            )
        }
        (true, None) => {
            quote!(
                let mut errors: Vec<tokio::task::JoinError> = vec![];
                #(
                    let res = tokio::spawn(#tasks).await;
                    if let Err(e) = res {
                        errors.push(res);
                    }
                )*
                if let Some(e) = errors.pop() {
                    std::panic::resume_unwind(e.into_panic())
                }
            )
        }
        (false, None) => {
            quote!(
                let results = ::suitest::internal::futures_util::future::join_all(
                    vec![
                        #(tokio::spawn(#tasks)),*
                    ]
                ).await;
                let mut errors: Vec<tokio::task::JoinError> = vec![];
                for (i, result) in results.into_iter().enumerate() {
                    if let Err(e) = result {
                        match i {
                            #(#ids => errors.push(e),)*
                            _ => unreachable!(),
                        }
                    }
                }
                if let Some(e) = errors.pop() {
                    std::panic::resume_unwind(e.into_panic())
                }
            )
        }
    }
}

/// Returns the new fn definition as the first element and the ident of that fn as the second. If the ident is `None`, the function
/// should not be invoked in the test suite (happens only the the `suite_fn` input argument is `None`).
///
/// The first element ultimately replaces the original fn, while the second is used when running the test suite.
fn quote_suite_fn(
    suite_fn: Option<&SuiteFn>,
    local: bool,
    print_msg: Option<&str>,
) -> (proc_macro2::TokenStream, Option<Ident>) {
    let Some(SuiteFn {
        item,
        modifier,
        inputs,
    }) = suite_fn
    else {
        return (quote!(), None);
    };

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = item;

    let block_stmts = &block.stmts;

    let state_getters = quote_state_getters(&sig.ident, inputs, local, print_msg.is_some());

    let state_setters = modifier
        .as_ref()
        .map(|modifier| quote_state_setters(&sig.ident, modifier, local, print_msg.is_some()));

    let state_get = (!state_getters.is_empty() || state_setters.is_some()).then_some(quote!(
        let state = STATE.get().expect("state not configured");
    ));

    let Signature {
        constness,
        asyncness,
        fn_token,
        ident,
        generics,
        ..
    } = sig;

    let tys = generics.type_params();
    let consts = generics.const_params();
    let local_id = local.then_some(quote!(const LOCAL_ID: usize,));
    let print = print_msg.map(|m| quote!(println!(#m);));

    (
        quote!(
            #(#attrs)*
            #vis #asyncness #constness #fn_token #ident < #local_id #(#consts)* #(#tys)* > ()  {
                #print
                #state_get
                #state_getters
                #(#block_stmts)*
                #state_setters
            }
        ),
        Some(ident.clone()),
    )
}

/// Use the original fn arguments to prepend state getters to the function block.
fn quote_state_getters(
    fn_id: &Ident,
    input: &Punctuated<FnArg, Comma>,
    local: bool,
    verbose: bool,
) -> proc_macro2::TokenStream {
    let mut tokens = quote!();
    input.pairs().map(|pair| pair.into_value()).for_each(|val| {
        let FnArg::Typed(pt) = val else {
            abort!(val.span(), "suitest functions cannot take in `self`")
        };

        let (Pat::Ident(id), ty) = (&*pt.pat, &pt.ty) else {
            abort!(
                val.span(),
                "suitest functions accept only `id: type` pairs, e.g. `foo: T`"
            )
        };

        let bucket = if local {
            quote!(LOCAL_ID)
        } else {
            quote!(GLOBAL)
        };

        let ty_display = type_display(None, ty);
        let printed = format!(
            "{fn_id} - getting {} from {} state",
            ty_display,
            if local { "local" } else { "global" }
        );
        let print = verbose.then_some(quote!(println!(#printed);));

        let expect = format!("unitialised item '{ty_display}' at '{fn_id}'");
        tokens.extend(quote!(
            #print
            let #id = state.get::<#bucket, #ty>().await.expect(#expect);
        ))
    });
    tokens
}

/// Use the state modifiers to append statements to the function block that insert the specified data to the state.
fn quote_state_setters(
    fn_id: &Ident,
    modifier: &StateModifier,
    local: bool,
    verbose: bool,
) -> proc_macro2::TokenStream {
    let bucket_id = if local {
        quote!(LOCAL_ID)
    } else {
        quote!(GLOBAL)
    };

    match (&modifier.fn_output, &modifier.last_block_item) {
        (PathOrTupleReturn::Path(ret_path), PathOrTupleExpr::Path(expr_path)) => {
            if !verbose {
                return quote!(
                    state.insert::<#bucket_id, #ret_path>(#expr_path).await;
                );
            }
            use std::fmt::Write;
            let mut result = String::new();
            for (i, seg) in ret_path.path.segments.iter().enumerate() {
                if i == ret_path.path.segments.len() - 1 {
                    write!(result, "{}", seg.ident).unwrap()
                } else {
                    write!(result, "{}::", seg.ident).unwrap()
                }
            }
            let printed = format!(
                "{fn_id} - getting {result} from {} state",
                if local { "local" } else { "global" }
            );
            let printed = quote!(println!(#printed););
            quote!(
                #printed
                state.insert::<#bucket_id, #ret_path>(#expr_path).await;
            )
        }
        (PathOrTupleReturn::Tuple(ret_tup), PathOrTupleExpr::Tuple(expr_tup)) => {
            let ty_elems = ret_tup
                .elems
                .pairs()
                .map(|pair| pair.into_value())
                .collect::<Vec<_>>();
            let val_elems = expr_tup
                .elems
                .pairs()
                .map(|pair| pair.into_value())
                .collect::<Vec<_>>();

            if ty_elems.len() != val_elems.len() {
                abort!(modifier.span(), "return value mismatch")
            }

            let printed = ty_elems.iter().map(|el| {
                let ty_display = type_display(None, el);
                let msg = format!(
                    "{fn_id} - inserting {ty_display} to {} state",
                    if local { "local" } else { "global" }
                );
                quote!(println!(#msg);)
            });

            if verbose {
                quote!(
                    #(
                        #printed
                        state.insert::<#bucket_id, #ty_elems>(#val_elems).await;
                    )*
                )
            } else {
                quote!(
                    #(
                        state.insert::<#bucket_id, #ty_elems>(#val_elems).await;
                    )*
                )
            }
        }
        _ => {
            abort!(modifier.span(), "return value mismatch")
        }
    }
}

/// Generates new test functions with the inputs removed and the state getters configured.
fn quote_test_declarations(tests: &[TestFn], verbose: bool) -> proc_macro2::TokenStream {
    let mut tokens = quote!();
    tests.iter().for_each(|test| {
        let TestFn { item, inputs, .. } = test;
        let ItemFn {
            attrs,
            vis,
            sig,
            block,
        } = item;

        let block_stmts = &block.stmts;

        let Signature {
            constness,
            asyncness,
            fn_token,
            ident,
            generics,
            ..
        } = sig;

        let tys = generics.type_params();
        let consts = generics.const_params();

        let state_getters = quote_state_getters(ident, inputs, true, verbose);
        let state_get = (!state_getters.is_empty()).then_some(quote!(
            let state = STATE.get().expect("state not configured");
        ));

        let new_attrs = attrs.iter().filter(|attr|!attr.meta.path().is_ident("test"));

        let msg = format!("Running test - {ident}");
        let print = verbose.then_some(quote!(println!(#msg);));

        let toks =quote!(
            #(#new_attrs)*
            #vis #asyncness #constness #fn_token #ident < const LOCAL_ID: usize, #(#consts)* #(#tys)* > ()  {
                #print
                #state_get
                #state_getters
                #(#block_stmts)*
            }
        );

        tokens.extend(toks);
    });

    tokens
}

fn type_display(prefix: Option<&str>, ty: &syn::Type) -> String {
    use std::fmt::Write;
    match ty {
        syn::Type::Array(arr) => type_display(Some("array of "), &arr.elem),
        syn::Type::Group(g) => type_display(Some("group of "), &g.elem),
        syn::Type::Paren(p) => type_display(None, &p.elem),
        syn::Type::Path(p) => {
            let mut result = prefix.unwrap_or_default().to_string();
            for (i, seg) in p.path.segments.iter().enumerate() {
                if i == p.path.segments.len() - 1 {
                    write!(result, "{}", seg.ident).unwrap()
                } else {
                    write!(result, "{}::", seg.ident).unwrap()
                }
            }
            result
        }
        syn::Type::Ptr(p) => type_display(Some("pointer to "), &p.elem),
        syn::Type::Reference(r) => type_display(Some("reference to "), &r.elem),
        syn::Type::Slice(s) => type_display(Some("slice of "), &s.elem),
        syn::Type::Tuple(t) => {
            let mut result = prefix.unwrap_or_default().to_string();
            for ty in t.elems.iter() {
                let ty_str = type_display(None, ty);
                write!(result, "{ty_str}").unwrap();
            }
            result
        }
        _ => abort!(ty.span(), "type cannot be used in suitest hook"),
    }
}
