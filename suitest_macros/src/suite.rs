use proc_macro_error::abort;
use syn::{
    parse::ParseBuffer, punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Expr,
    ExprLit, ExprPath, ExprTuple, FnArg, Ident, ItemFn, Lit, MetaNameValue, ReturnType, Token,
    TypePath, TypeTuple,
};

// Suite markers
const TEST: &str = "test";
const BEFORE_ALL: &str = "before_all";
const AFTER_ALL: &str = "after_all";
const BEFORE_EACH: &str = "before_each";
const AFTER_EACH: &str = "after_each";
const CLEANUP: &str = "cleanup";

// Configuration markers
const VERBOSE: &str = "verbose";
const SEQUENTIAL: &str = "sequential";

#[derive(Debug)]
pub struct TestSuite {
    /// The identifier used when invoking the `suite` macro.
    pub suite_id: Ident,

    /// The configuration used when invoking the `suite_cfg` macro.
    pub config: SuiteConfig,

    /// The test functions to run in the suite.
    pub tests: Vec<TestFn>,

    /// The first function that runs in the test suite.
    pub before_all: Option<SuiteFn>,

    /// The function that runs before each test in the test suite.
    pub before_each: Option<SuiteFn>,

    /// The last function that runs in the test suite.
    pub after_all: Option<SuiteFn>,

    /// The function that runs after each test in the test suite.
    pub after_each: Option<SuiteFn>,

    /// The function to call after a test fails.
    pub cleanup: Option<SuiteFn>,
}

impl TestSuite {
    pub fn new(id: Ident, config: SuiteConfig) -> Self {
        Self {
            suite_id: id,
            config,
            tests: vec![],
            before_all: None,
            before_each: None,
            after_all: None,
            after_each: None,
            cleanup: None,
        }
    }

    pub fn process_fn(&mut self, id: &mut usize, mut item: ItemFn) {
        // Remove the function arguments for the final function as we will
        // be collecting these from the state
        let inputs = std::mem::take(&mut item.sig.inputs);

        for attr in item.attrs.iter() {
            let Some(attr_ident) = attr.path().segments.last() else {
                continue;
            };

            match attr_ident.ident.to_string().as_str() {
                TEST => {
                    self.tests.push(TestFn::new(*id, item, inputs));
                    *id += 1;
                    break;
                }
                BEFORE_ALL => {
                    let modifier = Self::extract_fn_modifier(&mut item);
                    self.before_all = Some(SuiteFn::new(item, inputs));
                    if let Some(modifier) = modifier {
                        self.before_all.as_mut().unwrap().set_modifier(modifier);
                    }
                    break;
                }
                BEFORE_EACH => {
                    let modifier = Self::extract_fn_modifier(&mut item);
                    self.before_each = Some(SuiteFn::new(item, inputs));
                    if let Some(modifier) = modifier {
                        self.before_each.as_mut().unwrap().set_modifier(modifier);
                    }
                    break;
                }
                AFTER_ALL => {
                    self.after_all = Some(SuiteFn::new(item, inputs));
                    break;
                }
                AFTER_EACH => {
                    self.after_each = Some(SuiteFn::new(item, inputs));
                    break;
                }
                CLEANUP => {
                    self.cleanup = Some(SuiteFn::new(item, inputs));
                    break;
                }
                _ => {}
            }
        }
    }

    /// Extract the fn's return value and last block statement into a state modifier. Modifies the original item by
    /// removing its return value and popping the last statement in the function block.
    fn extract_fn_modifier(item: &mut ItemFn) -> Option<StateModifier> {
        let fn_output = match item.sig.output {
            // If the fn does not return anything, it will not modify the test suite state
            ReturnType::Default => {
                return None;
            }
            ReturnType::Type(_, ref ty) => match **ty {
                syn::Type::Path(ref p) => PathOrTupleReturn::Path(p.clone()),
                syn::Type::Tuple(ref t) => PathOrTupleReturn::Tuple(t.clone()),
                _ => abort!(
                    ty.span(),
                    "before_* hooks must return owned values (or tuples of)"
                ),
            },
        };

        // Skip if empty
        let last_stmt = item.block.stmts.pop()?;
        let last_block_item = match last_stmt {
            syn::Stmt::Expr(ref expr, tok) if tok.is_none() => match expr {
                syn::Expr::Path(ref p) => PathOrTupleExpr::Path(p.clone()),
                syn::Expr::Tuple(ref t) => PathOrTupleExpr::Tuple(t.clone()),
                _ => abort!(
                    last_stmt.span(),
                    "before_* hooks must return owned values (or tuples of)"
                ),
            },

            // Do not abort if the fn does not return anything
            _ if matches!(item.sig.output, ReturnType::Default) => return None,

            // We only accept expressions on before_* hooks
            _ => abort!(
                last_stmt.span(),
                "before_* hooks must return owned values (or tuples of)"
            ),
        };

        // Modify the output since we popped the last expression in the block
        item.sig.output = ReturnType::Default;

        Some(StateModifier {
            fn_output,
            last_block_item,
        })
    }
}

/// Represents the leftover hook function after extracting the state getters and setters.
#[derive(Debug)]
pub struct SuiteFn {
    /// The remaining fn item after the inputs and modifiers have been removed
    pub item: ItemFn,

    /// The extracted state modifiers from the original fn
    pub modifier: Option<StateModifier>,

    /// The extracted state getters from the original fn
    pub inputs: Punctuated<FnArg, Comma>,
}

impl SuiteFn {
    fn new(item: ItemFn, inputs: Punctuated<FnArg, Comma>) -> Self {
        Self {
            item,
            modifier: None,
            inputs,
        }
    }

    fn set_modifier(&mut self, modifier: StateModifier) {
        self.modifier = Some(modifier);
    }
}

/// A test function.
#[derive(Debug)]
pub struct TestFn {
    /// The test identifier
    pub id: usize,

    /// The function item with its inputs stripped
    pub item: ItemFn,

    /// The stripped inputs
    pub inputs: Punctuated<FnArg, Comma>,
}

impl TestFn {
    fn new(id: usize, item: ItemFn, inputs: Punctuated<FnArg, Comma>) -> Self {
        Self { id, item, inputs }
    }
}

/// Configuration for the test suite.
#[derive(Debug, Default)]
pub struct SuiteConfig {
    /// If true, the test suite prints all generated actions
    pub verbose: bool,

    /// If true, the test suite executes tests one by one
    pub sequential: bool,
}

impl SuiteConfig {
    /// If the suite is annotated with `suite_cfg`, this will parse it and return the configuration.
    pub fn parse(attrs: &[Attribute]) -> Self {
        let mut config = Self::default();

        for attr in attrs.iter() {
            let meta_list = attr.meta.require_list().unwrap();
            if meta_list
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "suite_cfg")
            {
                let args = meta_list
                    .parse_args_with(|buf: &ParseBuffer<'_>| {
                        Punctuated::<MetaNameValue, Token![,]>::parse_terminated(buf)
                    })
                    .unwrap();

                for arg in args {
                    let key = arg
                        .path
                        .require_ident()
                        .expect("invalid parameter passed to `suite_cfg`");

                    match key.to_string().as_str() {
                        VERBOSE => {
                            let Expr::Lit(ExprLit {
                                lit: Lit::Bool(bool),
                                ..
                            }) = arg.value
                            else {
                                abort!(arg.value, "verbose flag must be a boolean")
                            };
                            config.verbose = bool.value();
                        }
                        SEQUENTIAL => {
                            let Expr::Lit(ExprLit {
                                lit: Lit::Bool(bool),
                                ..
                            }) = arg.value
                            else {
                                abort!(arg.value, "sequential flag must be a boolean")
                            };
                            config.sequential = bool.value();
                        }

                        _ => abort!(arg.span(), "unrecognised argument"),
                    }
                }
            }
        }

        config
    }
}

/// The accepted values found at the function signature
#[derive(Debug)]
pub enum PathOrTupleReturn {
    Path(TypePath),
    Tuple(TypeTuple),
}

/// The accepted values found at the end of a function block
#[derive(Debug)]
pub enum PathOrTupleExpr {
    Path(ExprPath),
    Tuple(ExprTuple),
}

/// An intermediary repr of a suite function that should modify the test suite state
#[derive(Debug)]
pub struct StateModifier {
    /// The tuple or path from the function return value that gets used to insert the corresponding type to the state.
    pub fn_output: PathOrTupleReturn,

    /// The tuple or path from the function block that gets used to insert the corresponding type to the state.
    /// The type must correspond to `fn_output`.
    pub last_block_item: PathOrTupleExpr,
}

impl StateModifier {
    pub fn span(&self) -> proc_macro2::Span {
        match self.last_block_item {
            PathOrTupleExpr::Path(ref p) => p.span(),
            PathOrTupleExpr::Tuple(ref t) => t.span(),
        }
    }
}
