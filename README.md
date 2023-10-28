# suitest

A library that provides procedural macros for easily setting up test hooks and configuring test states.

## Example

```rust
#![cfg(test)]

use suitest::{suite, suite_cfg};

#[suite(my_test_suite)]
#[suite_cfg(sequential = false, verbose = true)]
pub mod tests {
    use suitest::{after_all, after_each, before_all, before_each, cleanup};

    #[derive(Debug, Clone)]
    struct MyTestStruct {
        qux: usize,
    }

    #[cleanup]
    fn cleaning() {
        println!("cleaning");
    }

    #[before_all]
    async fn setup() -> (usize, MyTestStruct) {
        (420_usize, MyTestStruct { qux: 69 })
    }

    #[before_each]
    fn beach() -> (u8, String) {
        (69_u8, String::from("works"))
    }

    #[after_each]
    fn aeach(works: String) {
        assert_eq!(works, "works")
    }

    #[after_all]
    async fn teardown(bar: usize, my_stuff: MyTestStruct) {
        assert_eq!(bar, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    fn works(works: String, foo: u8, bar: usize, my_stuff: MyTestStruct) {
        assert_eq!(works, "works");
        assert_eq!(foo, 69);
        assert_eq!(bar, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    async fn works_too(works: String, foo: u8, bar: usize, my_stuff: MyTestStruct) {
        assert_eq!(works, "works");
        assert_eq!(foo, 69);
        assert_eq!(bar, 420);
        assert_eq!(my_stuff.qux, 69);
    }
}

```

## How it works

Annotating the module you want as the test suite with `suitest::suite` lets you annotate the functions inside it with hooks.

Annotating it with `suitest::suite_cfg` and passing the parameters configures the suite.

Mark tests with `#[test]` as you normally would, suitest uses these annotations to register the functions as tests in the generated test suite. A single test will be generated at the end that runs the suite.

The available hooks are:

- `before_all`
  - Runs at the beginning of the suite
- `before_each`
  - Runs before each test in the suite
- `after_each`
  - Runs after each test in the suite
- `after_all`
  - Runs after the whole suite has passed
- `cleanup`
  - Runs after any test fails. Ideally should never panic.

suitest works with async functions, but depends on tokio so you need to have it in your dependencies. Any hook and test can be marked as async and you are allowed to mix and match, i.e. use async hooks with sync tests and vice versa.

### Config

`suite_cfg` is optional and accepts the following:

- `sequential = bool [false]`
  - Run the tests one after the other if true. Sync tests are executed before async tests.
    All tests in the suite are always executed regardless.
- `verbose = bool [false]`
  - Print what suitest is doing under the hood, useful for debugging.

### State

The test suite consist of 2 types of state; The global state which is available in the whole test suite and the local state which is local to tests.

The `*_all` are the only hooks that can mutate items in the global state. Tests and local hooks can only read from it.

The `*_each` hooks can mutate items in the local test states. Tests each get their own copy of the state provided in these hooks, which is also read only.

To add items to the states described above, all you need to do is make the hooks return an arbitrary tuple. This will generate code that inserts the provided parameters into the states.

To get the hold of the items, add the necessary parameters to the function's arguments. Local states have priority over the global one.

Each state can hold a single value per type. This means that if you insert any type more than once in the state, only the last entry will be in the map. If you need to insert multiple values of the same type, use a tuple.

Every hook and test will always attempt to retrieve items from its local state before trying to retrieve it from the global. The test/hook panics if it cannot find it in neither.
