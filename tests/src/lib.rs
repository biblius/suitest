#![cfg(test)]
use suitest::suite;

#[suite(my_test_suite)]
pub mod tests {
    use suitest::{after_all, after_each, before_all, before_each};

    #[derive(Debug, Clone)]
    struct MyTestStruct {
        qux: usize,
    }

    #[before_all]
    async fn setup() -> (usize, MyTestStruct) {
        (420, MyTestStruct { qux: 69 })
    }

    #[before_each]
    async fn beach() -> (u8, String) {
        (69, String::from("works"))
    }

    #[test]
    async fn works(works: String, magic: u8, chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(works, "works");
        assert_eq!(magic, 69);
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[after_each]
    async fn aeach(works: String) {
        assert_eq!(works, "works")
    }

    #[after_all]
    async fn teardown(chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }
}

/// Module used as a reference to see the internals of `suitest`.
pub mod prototype_with_maps {
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        future::Future,
        pin::Pin,
    };

    use suitest::internal::once_cell::sync::Lazy;

    type AnyMap = HashMap<TypeId, Box<dyn Any + Send + Sync>>;

    const ID0: usize = 0;
    const ID1: usize = 1;
    const ID2: usize = 2;
    static mut GLOBAL: Lazy<AnyMap> = Lazy::new(HashMap::new);
    static mut LOCAL: [Lazy<AnyMap>; 3] = [
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
        suitest::internal::once_cell::sync::Lazy::new(HashMap::new),
    ];

    fn before_all() {
        // SAFETY: We know this runs only at the beginning of the suite, where nothing could have
        // gotten a reference to GLOBAL.
        unsafe { GLOBAL.insert(TypeId::of::<usize>(), Box::new(69_usize)) };
        println!("Running b_all");
    }

    fn before_each<const ID: usize>() {
        // SAFETY: We know each test and its respective hook gets assigned a unique ID, meaning
        // there is no way to hold 2 mutable references to the same map. The hooks and test are always ran sequentially,
        // so they cannot access the map at the same time.
        let state = unsafe { &mut LOCAL[ID] };
        state.insert(TypeId::of::<usize>(), Box::new(420_usize));
        println!("Running b_each");
    }

    fn after_all() {
        println!("Running a_all");
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

    fn after_each() {
        println!("Running a_each");
    }

    async fn foo<const ID: usize>() {
        println!("Running foo {ID}");
        let state = unsafe { &mut LOCAL[ID] };
        let num = state
            .get(&TypeId::of::<usize>())
            .expect("item not in state")
            .downcast_ref::<usize>()
            .expect("the impossible");
        assert_eq!(*num, 420_usize);
    }

    fn bar() {
        println!("Running bar");
        // panic!("bar");
    }

    #[test]
    fn works() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("couldn't build runtime");

        before_all();

        let mut task0 = async move {
            before_each::<ID0>();
            foo::<ID0>().await;
            after_each();
        };

        let mut task1 = async move {
            before_each::<ID1>();
            foo::<ID1>().await;
            after_each();
        };

        let mut task2 = async move {
            before_each::<ID2>();
            foo::<ID2>().await;
            after_each();
        };

        rt.block_on(suitest::internal::futures_util::future::join_all(vec![
            unsafe { Pin::new_unchecked(&mut task0) } as Pin<&mut (dyn Future<Output = ()> + Send)>,
            unsafe { Pin::new_unchecked(&mut task1) },
            unsafe { Pin::new_unchecked(&mut task2) },
        ]));

        after_all();
        /*
        let mut handles = vec![];
        for _ in 0..3 {
            let thread = std::thread::Builder::new().name("bar".to_string());
            handles.push(thread.spawn(bar).expect("couldn't spawn thread"))
        }
        for t in handles {
            let e = t.join();
            if let Err(e) = e {
                dbg!(&e);
                std::panic::resume_unwind(e);
            }
            // dbg!(e);
        } */
    }
}
