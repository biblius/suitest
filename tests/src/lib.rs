#![cfg(test)]

use suitest::{suite, suite_cfg};

mod poc;

#[suite(my_test_suite)]
#[suite_cfg(sequential = true, verbose = false)]
pub mod tests {
    use suitest::{after_all, after_each, before_all, before_each};

    #[derive(Debug, Clone)]
    struct MyTestStruct {
        qux: usize,
    }

    #[suitest::cleanup]
    fn cleaning() {}

    #[before_all]
    async fn setup() -> (usize, MyTestStruct) {
        (420_usize, MyTestStruct { qux: 69 })
    }

    #[before_each]
    fn beach() -> (u8, (String, String)) {
        (69_u8, (String::from("works"), String::from("tuple")))
    }

    #[after_each]
    fn aeach(works: (String, String)) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
    }

    #[after_all]
    async fn teardown(chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    fn works(works: (String, String), magic: u8, chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
        assert_eq!(magic, 69);
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    fn works_too(works: (String, String), magic: u8, chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
        assert_eq!(magic, 69);
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }
}

#[suite(my_test_suite)]
#[suite_cfg(sequential = true, verbose = false)]
pub mod async_tests {
    use suitest::{after_all, after_each, before_all, before_each};

    #[derive(Debug, Clone)]
    struct MyTestStruct {
        qux: usize,
    }

    #[suitest::cleanup]
    async fn cleaning() {}

    #[before_all]
    async fn setup() -> (usize, MyTestStruct) {
        (420_usize, MyTestStruct { qux: 69 })
    }

    #[before_each]
    async fn beach() -> (u8, (String, String)) {
        (69_u8, (String::from("works"), String::from("tuple")))
    }

    #[after_each]
    async fn aeach(works: (String, String)) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
    }

    #[after_all]
    async fn teardown(chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    async fn works(works: (String, String), magic: u8, chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
        assert_eq!(magic, 69);
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }

    #[test]
    async fn works_too(works: (String, String), magic: u8, chosen: usize, my_stuff: MyTestStruct) {
        assert_eq!(works.0, "works");
        assert_eq!(works.1, "tuple");
        assert_eq!(magic, 69);
        assert_eq!(chosen, 420);
        assert_eq!(my_stuff.qux, 69);
    }
}
