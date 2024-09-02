#![allow(dead_code, unused_imports)]

/// Module used as a reference to see the internals of `suitest`.
pub mod prototype_with_maps {
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        future::Future,
        pin::Pin,
    };

    use suitest::internal::{futures_util::FutureExt, once_cell::sync::Lazy};

    type AnyMap = HashMap<TypeId, Box<dyn Any + Send + Sync>>;

    const ID0: usize = 0;
    const ID1: usize = 1;
    const ID2: usize = 2;
    const ID3: usize = 3;
    static mut GLOBAL: Lazy<AnyMap> = Lazy::new(HashMap::new);
    static mut LOCAL: [Lazy<AnyMap>; 4] = [
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
    ];

    fn before_all() {
        // SAFETY: We know this runs only at the beginning of the suite, where nothing could have
        // gotten a reference to GLOBAL.
        unsafe {
            GLOBAL.insert(TypeId::of::<usize>(), Box::new(69_usize));
            GLOBAL.insert(TypeId::of::<String>(), Box::new(String::from("foo")));
        };
    }

    fn before_each<const ID: usize>() {
        // SAFETY: We know each test and its respective hooks gets assigned a unique ID, meaning
        // there is no way to hold 2 mutable references to the same map. The hooks and test are always ran sequentially,
        // so they cannot access the map at the same time.
        let state = unsafe { &mut LOCAL[ID] };
        state.insert(TypeId::of::<usize>(), Box::new(420_usize));
    }

    fn after_all() {
        // SAFETY: Same as before_all, except it happens at the end of the suite where we've joined all handles
        // that can potentially hold references to the global state.
        let num = unsafe {
            *GLOBAL
                .remove(&TypeId::of::<usize>())
                .unwrap()
                .downcast::<usize>()
                .unwrap()
        };
        assert_eq!(num, 69);
    }

    fn after_each<const ID: usize>() {
        // SAFETY: Local IDs are fine because they are sequential.
        // We know the global state contains no mutable references because
        // this always runs in between the *_all hooks.
        // Tests and *_each hooks never mutably access the global state.
        let ftw = unsafe {
            LOCAL[ID]
                .get(&TypeId::of::<String>())
                .or_else(|| GLOBAL.get(&TypeId::of::<String>()))
                .expect("item could not be found in state")
                .downcast_ref::<String>()
                .expect("the impossible")
        };
        assert_eq!(ftw, "foo");
    }

    async fn test_foo<const ID: usize>() {
        // SAFETY: We know this is the only test that can access the map under the index ID.
        // Since hooks do not run while this test is running, nothing else is touching the map
        // which means we're good to go.
        let state = unsafe { &mut LOCAL[ID] };
        let num = state
            .get(&TypeId::of::<usize>())
            .expect("item not in state")
            .downcast_ref::<usize>()
            .expect("the impossible");
        assert_eq!(*num, 420_usize);
    }

    async fn cleanup<const ID: usize>() {
        let state = unsafe { &mut LOCAL[ID] };
        let num = state
            .get(&TypeId::of::<usize>())
            .expect("item not in state")
            .downcast_ref::<usize>()
            .expect("the impossible");
        assert_eq!(*num, 420_usize);
    }

    fn test_bar<const ID: usize>() {
        // panic!("test_bar");
    }

    #[test]
    fn works() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("couldn't build runtime");

        before_all();

        let task0 = async {
            before_each::<ID0>();
            test_foo::<ID0>().await;
            after_each::<ID0>();
        };

        let task1 = async {
            before_each::<ID1>();
            test_foo::<ID1>().await;
            after_each::<ID1>();
        };

        let task2 = async {
            before_each::<ID2>();
            test_foo::<ID2>().await;
            after_each::<ID2>();
        };

        let task4 = || {
            before_each::<ID3>();
            test_bar::<ID3>();
            after_each::<ID3>();
        };

        let mut errors = vec![];

        // SEQ ASYNC

        use suitest::internal::futures_util;
        let result = rt.block_on(async { tokio::spawn(task0).await });
        if let Err(e) = result {
            errors.push(e.into_panic());
        }

        let result = rt.block_on(task1.catch_unwind());
        if let Err(e) = result {
            errors.push(e);
        }

        let result = rt.block_on(task2.catch_unwind());
        if let Err(e) = result {
            errors.push(e);
        }

        // SEQ ASYNC END

        // PARALLEL ASYNC

        /*         let join_result = rt.block_on(async {
            suitest::internal::futures_util::future::join_all(vec![
                tokio::spawn(task0),
                tokio::spawn(task1),
                tokio::spawn(task2),
            ])
            .await
        });

        for res in join_result {
            if let Err(e) = res {
                errors.push(e.into_panic());
            }
        } */

        // PARALLEL ASYNC END

        // SEQ

        let result = std::panic::catch_unwind(task4);
        if let Err(e) = result {
            errors.push(e);
        }

        // SEQ END

        // PARALLEL

        let mut handles = vec![];

        for _ in 0..1 {
            let thread = std::thread::Builder::new().name("test_bar".to_string());
            handles.push(thread.spawn(task4).expect("couldn't spawn thread"))
        }

        for t in handles {
            let res = t.join();
            if let Err(e) = res {
                errors.push(e);
            }
        }

        // PARALLEL END

        if let Some(e) = errors.pop() {
            std::panic::resume_unwind(e);
        }

        after_all();
    }
}
