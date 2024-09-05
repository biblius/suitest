use crate::suite::{
    FnQuote, PathOrTupleExpr, PathOrTupleReturn, StateModifier, SuiteConfig, SuiteFn, TaskQuote,
    TestFn, TestSuite,
};
use proc_macro_error::abort;
use quote::{format_ident, quote};
use syn::{
    punctuated::Punctuated, spanned::Spanned, token::Comma, Expr, FnArg, Ident, ItemFn, ItemMod,
    Pat, Signature, Type,
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

    // Parse top level `suite_cfg`.
    let config = SuiteConfig::parse(&item_mod.attrs);

    let mut suite = TestSuite::new(id, config);

    // Identifiers for local test states
    let mut i = 0;

    for item in items {
        suite.process_item(&mut i, item)
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
        is_async,
        other_items: other,
    } = suite;

    // If any of the fns are async a tokio runtime needs to be spawned
    let runtime = is_async.then_some(quote!(
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("couldn't build runtime");
    ));

    // Used as identifiers for the tests and their hooks
    let ids = tests
        .iter()
        .map(|t| format_ident!("ID{}", t.id))
        .collect::<Vec<_>>();

    // Used as actual usize literals for the const generics
    let id_lits = tests.iter().map(|t| t.id).collect::<Vec<_>>();

    let len = tests.len();

    let maps = {
        let mut v = vec![];
        for _ in 0..len {
            v.push(quote!(suitest::internal::once_cell::sync::Lazy::new(
                ::std::collections::HashMap::new
            ),));
        }
        v
    };
    let local_map = quote!(static mut __LOCAL: [suitest::internal::once_cell::sync::Lazy<__AnyMap>; #len] = [#(#maps)*];);

    let verbose = config.verbose;

    let before_all = quote_suite_fn(
        before_all.as_ref(),
        false,
        verbose.then_some("Running before_all"),
    );

    let after_all = quote_suite_fn(
        after_all.as_ref(),
        false,
        verbose.then_some("Running after_all"),
    );

    let before_each = quote_suite_fn(
        before_each.as_ref(),
        true,
        verbose.then_some("Running before_each"),
    );

    let after_each = quote_suite_fn(
        after_each.as_ref(),
        true,
        verbose.then_some("Running after_each"),
    );

    let cleanup = quote_suite_fn(cleanup.as_ref(), true, verbose.then_some("Running cleanup"));

    let ba_invoke = before_all.as_ref().map(FnQuote::quote_invoke_suite);
    let aa_invoke = after_all.as_ref().map(FnQuote::quote_invoke_suite);

    let test_declarations = quote_test_declarations(&tests, verbose);

    let mut test_tasks = quote!();
    let mut tasks_sync = vec![];
    let mut tasks_async = vec![];

    for (test, const_id) in tests.iter().zip(ids.iter()) {
        let TestFn { id, item, .. } = test;

        let test_id = &item.sig.ident;
        let is_async = item.sig.asyncness.is_some();

        let be_invoke = before_each
            .as_ref()
            .map(|fq| fq.quote_invoke_task(const_id));

        let ae_invoke = after_each.as_ref().map(|fq| fq.quote_invoke_task(const_id));

        let test_invoke = if is_async {
            quote!(#test_id::<#const_id>().await;)
        } else {
            quote!(#test_id::<#const_id>();)
        };

        let is_async = is_async
            || before_each.as_ref().is_some_and(|f| f.is_async)
            || after_each.as_ref().is_some_and(|f| f.is_async);

        let closure = if is_async { quote!(async) } else { quote!(||) };

        let task_id = format_ident!("test{id}");

        let msg = format!("{} ... {}", item.sig.ident, "\u{1F5F8}");
        let msg = quote!(println!(#msg));

        let tokens = quote!(
            let #task_id = #closure {
                #be_invoke
                #test_invoke
                #ae_invoke
                #msg
            };
        );

        let task = TaskQuote::new(
            task_id,
            item.sig.ident.clone(),
            const_id.clone(),
            cleanup.as_ref().map(|f| (f.id.clone(), f.is_async)),
        );

        if is_async {
            tasks_async.push(task);
        } else {
            tasks_sync.push(task);
        }

        test_tasks.extend(tokens);
    }

    // Call sync implementations.
    let exec_sync = (!tasks_sync.is_empty()).then(|| {
        if config.sequential {
            quote_seq_exec_sync(&tasks_sync)
        } else {
            quote_par_exec_sync(&tasks_sync)
        }
    });

    // Call async implementations.
    let exec_async = (!tasks_async.is_empty()).then(|| {
        if config.sequential {
            quote_seq_exec_async(&tasks_async)
        } else {
            quote_par_exec_async(&tasks_async)
        }
    });

    // Used to trigger the drop implementations of what is stored in the state.
    // testcontainers is one example of where cleanup is sometimes necessary.
    let drop_state = if is_async {
        quote!(
            rt.block_on(rt.spawn(
                async {
                    // SAFETY: Nothing is touching the global state at this point.
                    let items = unsafe { __GLOBAL.drain() };
                    drop(items);
                }
            )).expect("unable to spawn drop task");
        )
    } else {
        quote!(
            // SAFETY: Nothing is touching the global state at this point.
            let items = unsafe { __GLOBAL.drain() };
            drop(items);
        )
    };

    quote!(
        #(#attrs)*
        #vis #mod_token #ident {
            #(#other)*

            type __AnyMapValue = ::std::boxed::Box<dyn ::std::any::Any + Send + Sync>;
            type __AnyMap = ::std::collections::HashMap<::std::any::TypeId, __AnyMapValue>;
            type __LazyMap = ::suitest::internal::once_cell::sync::Lazy<__AnyMap>;
            static mut __GLOBAL: __LazyMap = __LazyMap::new(|| __AnyMap::new());

            #local_map

            #(const #ids: usize = #id_lits;)*

            #before_all

            #before_each

            #after_each

            #after_all

            #test_declarations

            #cleanup

            #[test]
            fn #suite_id () {
                #runtime

                #test_tasks

                #ba_invoke

                let mut errors: Vec<Box<dyn ::std::any::Any + Send + 'static>> = vec![];

                #exec_sync

                #exec_async

                if errors.is_empty() {
                    #aa_invoke
                }

                #drop_state

                if let Some(e) = errors.pop() {
                    ::std::panic::resume_unwind(e);
                }
            }
        }
    )
}

fn quote_seq_exec_async(tasks: &[TaskQuote]) -> proc_macro2::TokenStream {
    let mut tokens = quote!();
    for task in tasks {
        let id = &task.id;
        let fn_id = &task.fn_id.to_string();
        let const_id = &task.const_id;
        let cleanup = task.cleanup.as_ref().map(|(cleanup, is_async)| {
            if *is_async {
                quote!(rt.block_on(#cleanup::<#const_id>());)
            } else {
                quote!(#cleanup::<#const_id>();)
            }
        });
        tokens.extend(quote!(
            let result = rt.block_on(rt.spawn(#id));
            if let Err(e) = result {
                eprintln!("{} ... x", #fn_id);
                #cleanup
                errors.push(e.into_panic());
            }
        ));
    }
    tokens
}

fn quote_par_exec_async(tasks: &[TaskQuote]) -> proc_macro2::TokenStream {
    let spawns = tasks.iter().map(|t| {
        let id = &t.id;
        quote!(rt.spawn(::std::boxed::Box::pin(#id)),)
    });

    let msg = tasks.iter().map(|t| {
        let id = format!("{} ... x", &t.fn_id);
        quote!(#id)
    });

    let cleanups = tasks.iter().map(|t| {
        let const_id = &t.const_id;
        t.cleanup.as_ref().map(|(cleanup, is_async)| {
            let cleanup = if *is_async {
                quote!(rt.block_on(#cleanup::<#const_id>());)
            } else {
                quote!(#cleanup::<#const_id>();)
            };
            quote!(if i == #const_id { #cleanup })
        })
    });

    quote!(
        let results = rt.block_on(
            suitest::internal::futures_util::future::join_all(
                vec![#(#spawns)*]
            )
        );

        let msgs = [
            #(#msg),*
        ];

        for (i, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                eprintln!("{}", msgs[i]);
                #(#cleanups)*
                errors.push(e.into_panic());
            }
        }
    )
}

fn quote_par_exec_sync(tasks: &[TaskQuote]) -> proc_macro2::TokenStream {
    let task_invokes = tasks.iter().map(|t| {
        let (id, thread_id) = (&t.id, t.fn_id.to_string());
        quote!(
            let thread = ::std::thread::Builder::new().name(#thread_id.to_string());
            handles.push(thread.spawn(#id).expect("could not spawn test thread"));
        )
    });

    let const_ids = tasks.iter().map(|t| &t.const_id).collect::<Vec<_>>();
    let cleanups = tasks.iter().zip(const_ids.iter()).map(|(t, const_id)| {
        t.cleanup.as_ref().map(|(cleanup, is_async)| {
            if *is_async {
                quote!(rt.block_on(#cleanup::<#const_id>());)
            } else {
                quote!(#cleanup::<#const_id>();)
            }
        })
    });

    let msgs = tasks.iter().map(|t| {
        let msg = format!("{} ... x", t.fn_id);
        quote!(eprintln!(#msg);)
    });

    quote!(
        let mut handles = vec![];
        #(#task_invokes)*
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.join();
            if let Err(e) = result {
                match i {
                    #(
                        #const_ids => {
                            #msgs
                            #cleanups
                            errors.push(e);
                        }
                    )*
                    _ => unreachable!()
                }
            }
        }
    )
}

fn quote_seq_exec_sync(tasks: &[TaskQuote]) -> proc_macro2::TokenStream {
    let mut tokens = quote!();

    for task in tasks {
        let TaskQuote {
            id,
            const_id,
            cleanup,
            ..
        } = task;

        let cleanup = cleanup.as_ref().map(|(cleanup, is_async)| {
            if *is_async {
                quote!(rt.block_on(#cleanup::<#const_id>());)
            } else {
                quote!(#cleanup::<#const_id>();)
            }
        });

        let msg = format!("{} ... x", task.fn_id);
        let msg = quote!(println!(#msg););

        tokens.extend(quote!(
           let result = ::std::panic::catch_unwind(#id);
           if let Err(e) = result {
            #msg
            #cleanup
            errors.push(e);
           }
        ));
    }

    tokens
}

/// Returns the new fn definition as the first element and the ident of that fn as the second. If the ident is `None`, the function
/// should not be invoked in the test suite (happens only the the `suite_fn` input argument is `None`).
///
/// The first element ultimately replaces the original fn, while the second is used when running the test suite.
fn quote_suite_fn(
    suite_fn: Option<&SuiteFn>,
    local: bool,
    print_msg: Option<&str>,
) -> Option<FnQuote> {
    let SuiteFn {
        item,
        modifier,
        inputs,
    } = suite_fn?;

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

    let tokens = quote!(
        #(#attrs)*
        #vis #asyncness #constness #fn_token #ident < #local_id #(#consts)* #(#tys)* > ()  {
            #print
            #state_getters
            #(#block_stmts)*
            #state_setters
        }
    );

    Some(FnQuote::new(tokens, ident.clone(), sig.asyncness.is_some()))
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

        // Print statements
        let ty_display = type_display(None, ty);
        let expect = format!("unitialised item '{ty_display}' at '{fn_id}'");
        let local_miss =
            format!("{fn_id} - {ty_display} not found in local state, getting from global");
        let local_miss = verbose.then_some(quote!(println!(#local_miss);));

        let getters = if local {
            quote!(
                // SAFETY: Multiple immutable references are fine.
                // We are never modifying the __GLOBAL state other
                // than in *_all hooks, only reading from it.
                let #id: &#ty = unsafe {
                    let item = __LOCAL[LOCAL_ID]
                        .get(&::std::any::TypeId::of::<#ty>())
                        .or_else(|| {
                            #local_miss
                            __GLOBAL.get(&::std::any::TypeId::of::<#ty>())
                        })
                        .expect(#expect);
                    item
                        .downcast_ref::<#ty>()
                        .unwrap_or_else(||
                            panic!(
                                    "downcast to '{}' failed; expected '{:?}', found '{:?}'",
                                    #ty_display,
                                    ::std::any::TypeId::of::<#ty>(),
                                    item.type_id()
                            )
                        )
                };
            )
        } else {
            quote!(
                // SAFETY: Multiple immutable references are fine.
                // We are never modifying the global state other
                // than in *_all hooks, only reading from it.
                let #id: &#ty = unsafe {
                    let item = __GLOBAL
                        .get(&::std::any::TypeId::of::<#ty>())
                        .expect(#expect);
                    item
                        .downcast_ref::<#ty>()
                        .unwrap_or_else(||
                            panic!(
                                    "downcast to '{}' failed; expected '{:?}', found '{:?}'",
                                    #ty_display,
                                    ::std::any::TypeId::of::<#ty>(),
                                    item.type_id()
                            )
                        )
                };
            )
        };

        let printed = format!(
            "{fn_id} - getting {} from {} state",
            ty_display,
            if local { "local" } else { "global" }
        );
        let print = verbose.then_some(quote!(println!(#printed);));

        tokens.extend(quote!(
            #print
            #getters
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
    let state_map = if local {
        quote!(let state = unsafe { &mut __LOCAL[LOCAL_ID] };)
    } else {
        quote!(let state = unsafe { &mut __GLOBAL };)
    };

    match (&modifier.fn_output, &modifier.last_block_item) {
        (PathOrTupleReturn::Path(ret_path), PathOrTupleExpr::Path(expr_path)) => {
            use ::std::fmt::Write;

            let mut result = String::new();

            for (i, seg) in ret_path.path.segments.iter().enumerate() {
                if i == ret_path.path.segments.len() - 1 {
                    write!(result, "{}", seg.ident).unwrap()
                } else {
                    write!(result, "{}::", seg.ident).unwrap()
                }
            }

            let printed = format!(
                "{fn_id} - setting {result} to {} state",
                if local { "local" } else { "global" }
            );

            let printed = verbose.then_some(quote!(println!(#printed);));

            quote!(
            {
                #printed
                #state_map
                let boxed = Box::new(#expr_path);
                let type_id = ::std::any::TypeId::of::<#ret_path>();
                assert_eq!(type_id, <#ret_path as ::std::any::Any>::type_id(&*boxed), "type mismatch; check test hooks for correct types");
                state.insert(type_id, boxed);
            })
        }
        (PathOrTupleReturn::Tuple(ret_tup), PathOrTupleExpr::Tuple(expr_tup)) => {
            let ret_elems = ret_tup
                .elems
                .pairs()
                .map(|pair| pair.into_value())
                .collect::<Vec<&Type>>();

            let expr_elems = expr_tup
                .elems
                .pairs()
                .map(|pair| pair.into_value())
                .collect::<Vec<&Expr>>();

            if ret_elems.len() != expr_elems.len() {
                abort!(
                    modifier.span(),
                    "return value mismatch, expected {} items in return",
                    ret_elems.len()
                )
            }

            let printed = ret_elems.iter().map(|el| {
                if !verbose {
                    return None;
                }
                let ty_display = type_display(None, el);
                let msg = format!(
                    "{fn_id} - setting {ty_display} to {} state",
                    if local { "local" } else { "global" }
                );
                Some(quote!(println!(#msg);))
            });

            quote!(
                #state_map
                #({
                    #printed
                    let boxed = Box::new(#expr_elems);
                    let type_id = ::std::any::TypeId::of::<#ret_elems>();
                    assert_eq!(type_id, <#ret_elems as ::std::any::Any>::type_id(&*boxed), "type mismatch; check test hooks for correct types");
                    state.insert(type_id, boxed);
                })*
            )
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

        let new_attrs = attrs.iter().filter(|attr|!attr.meta.path().is_ident("test"));

        let msg = format!("{ident} - starting test");
        let print = verbose.then_some(quote!(println!(#msg);));

        let toks =quote!(
            #(#new_attrs)*
            #vis #asyncness #constness #fn_token #ident < const LOCAL_ID: usize, #(#consts)* #(#tys)* > ()  {
                #print
                #state_getters
                #(#block_stmts)*
            }
        );

        tokens.extend(toks);
    });

    tokens
}

fn type_display(prefix: Option<&str>, ty: &syn::Type) -> String {
    use ::std::fmt::Write;
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
