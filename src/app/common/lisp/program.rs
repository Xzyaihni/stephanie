use std::{
    vec,
    iter,
    rc::Rc,
    cell::RefCell,
    fmt::{self, Debug, Display},
    collections::HashMap,
    ops::{RangeInclusive, Add, Sub, Mul, Div, Rem, Deref, Index, IndexMut}
};

pub use super::{
    clone_with_capacity,
    Error,
    ErrorPos,
    LispValue,
    LispMemory,
    ValueTag,
    LispVectorRef,
    OutputWrapper
};

pub use parser::{CodePosition, WithPosition};

use parser::{Parser, Ast, AstPos, PrimitiveType};

mod parser;


// unreadable, great
pub type OnApply = Rc<
    dyn Fn(
        &State,
        &mut LispMemory,
        &ExpressionPos,
        Action
    ) -> Result<(), ErrorPos>>;

pub type OnEval = Rc<
    dyn Fn(
        ExpressionPos,
        &mut State,
        AstPos
    ) -> Result<ExpressionPos, ErrorPos>>;

pub trait Memoriable
{
    fn as_memory_mut(&mut self) -> &mut LispMemory;
}

impl Memoriable for LispMemory
{
    fn as_memory_mut(&mut self) -> &mut LispMemory
    {
        self
    }
}

#[derive(Debug)]
pub struct MemoryWrapper<'a>
{
    memory: &'a mut LispMemory,
    ignore_return: bool
}

impl<'a> Memoriable for MemoryWrapper<'a>
{
    fn as_memory_mut(&mut self) -> &mut LispMemory
    {
        self.memory
    }
}

impl MemoryWrapper<'_>
{
    pub fn push_return(&mut self, value: impl Into<LispValue>)
    {
        if self.ignore_return
        {
            return;
        }

        self.memory.push_return(value.into());
    }
}

pub fn simple_apply<const EFFECT: bool>(f: impl Fn(
    &State,
    &mut MemoryWrapper,
    ArgsWrapper
) -> Result<(), Error> + 'static) -> OnApply
{
    Rc::new(move |
        state: &State,
        memory: &mut LispMemory,
        args: &ExpressionPos,
        action: Action
    |
    {
        let position = args.position;

        let args = args.eval_args(state, memory, if EFFECT {
            Action::Return
        } else
        {
            action 
        })?;

        if !EFFECT && action == Action::None
        {
            return Ok(());
        }

        let mut memory = MemoryWrapper{memory, ignore_return: action == Action::None};

        f(state, &mut memory, args).with_position(position)
    })
}

pub struct ArgsWrapper
{
    count: usize
}

impl ArgsWrapper
{
    pub fn new() -> Self
    {
        Self{count: 0}
    }

    pub fn pop<'a>(&mut self, memory: &'a mut impl Memoriable) -> OutputWrapper<'a>
    {
        self.try_pop(memory).expect("pop must be called on argcount > 0")
    }

    pub fn try_pop<'a>(&mut self, memory: &'a mut impl Memoriable) -> Option<OutputWrapper<'a>>
    {
        if self.count == 0
        {
            return None;
        }

        self.count -= 1;

        let memory = memory.as_memory_mut();
        let value = memory.pop_return();

        Some(OutputWrapper{memory, value})
    }

    pub fn push(&mut self, memory: &mut LispMemory, value: LispValue)
    {
        self.count += 1;

        memory.push_return(value);
    }

    pub fn as_list(&mut self, memory: &mut LispMemory) -> LispValue
    {
        let args: Vec<_> = (0..self.count).map(|_| memory.pop_return()).collect();

        args.into_iter().for_each(|value|
        {
            memory.push_return(value);
        });

        memory.push_return(());
        (0..self.count).for_each(|_| memory.cons());

        self.count = 0;

        memory.pop_return()
    }

    pub fn clear(&mut self, memory: &mut LispMemory)
    {
        while !self.is_empty()
        {
            self.pop(memory);
        }
    }

    pub fn len(&self) -> usize
    {
        self.count
    }

    pub fn is_empty(&self) -> bool
    {
        self.count == 0
    }

    pub fn increment(mut self) -> Self
    {
        self.count += 1;

        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ArgsCount
{
    Min(usize),
    Between{start: usize, end_inclusive: usize},
    Some(usize)
}

impl Display for ArgsCount
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", match self
        {
            ArgsCount::Some(x) => x.to_string(),
            ArgsCount::Between{start, end_inclusive} => format!("between {start} and {end_inclusive}"),
            ArgsCount::Min(x) => format!("at least {x}")
        })
    }
}

impl From<RangeInclusive<usize>> for ArgsCount
{
    fn from(value: RangeInclusive<usize>) -> Self
    {
        Self::Between{start: *value.start(), end_inclusive: *value.end()}
    }
}

impl From<usize> for ArgsCount
{
    fn from(value: usize) -> Self
    {
        Self::Some(value)
    }
}

impl WithPosition for Expression
{
    type Output = ExpressionPos;

    fn with_position(self, position: CodePosition) -> Self::Output
    {
        ExpressionPos{
            position,
            expression: self
        }
    }
}

impl<T> WithPosition for Result<T, Error>
{
    type Output = Result<T, ErrorPos>;

    fn with_position(self, position: CodePosition) -> Self::Output
    {
        self.map_err(|error| ErrorPos{position, error})
    }
}

#[derive(Clone)]
pub struct PrimitiveProcedureInfo
{
    args_count: ArgsCount,
    on_eval: Option<OnEval>,
    on_apply: Option<OnApply>
}

impl PrimitiveProcedureInfo
{
    pub fn new_eval(
        args_count: impl Into<ArgsCount>,
        on_eval: OnEval
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(on_eval),
            on_apply: None
        }
    }

    pub fn new(
        args_count: impl Into<ArgsCount>,
        on_eval: OnEval,
        on_apply: OnApply
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(on_eval),
            on_apply: Some(on_apply)
        }
    }

    pub fn new_simple_lazy(args_count: impl Into<ArgsCount>, on_apply: OnApply) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some(on_apply)
        }
    }

    pub fn new_simple_effect<F>(
        args_count: impl Into<ArgsCount>,
        on_apply: F
    ) -> Self
    where
        F: Fn(
            &State,
            &mut MemoryWrapper,
            ArgsWrapper
        ) -> Result<(), Error> + 'static
    {
        let on_apply = simple_apply::<true>(on_apply);

        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some(on_apply)
        }
    }

    pub fn new_simple<F>(
        args_count: impl Into<ArgsCount>,
        on_apply: F
    ) -> Self
    where
        F: Fn(
            &State,
            &mut LispMemory,
            ArgsWrapper
        ) -> Result<(), Error> + 'static
    {
        let on_apply = simple_apply::<false>(move |state, memory, args|
        {
            on_apply(state, memory.memory, args)
        });

        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some(on_apply)
        }
    }
}

impl Debug for PrimitiveProcedureInfo
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "<procedure with {} args>", &self.args_count)
    }
}

#[derive(Debug, Clone)]
pub enum Either<A, B>
{
    Left(A),
    Right(B)
}

// i dont wanna store the body over and over in the virtual memory
// but this seems silly, so i dunno >~<
#[derive(Debug, Clone)]
pub struct StoredLambda
{
    pub parent_env: RefCell<LispValue>,
    params: Either<String, ArgValues<String>>,
    body: ExpressionPos
}

impl StoredLambda
{
    pub fn new(
        memory: &mut LispMemory,
        params: ExpressionPos,
        body: ExpressionPos
    ) -> Result<Self, ErrorPos>
    {
        let params = match params.as_value()
        {
            Ok(x) => Either::Left(x),
            Err(_) => Either::Right(params.map_list(|arg|
            {
                arg.as_value()
            })?)
        };

        Ok(Self{parent_env: RefCell::new(memory.env), params, body})
    }

    pub fn apply(
        &self,
        state: &State,
        memory: &mut LispMemory,
        mut args: ArgsWrapper,
        action: Action
    ) -> Result<(), ErrorPos>
    {
        let parent_env = *self.parent_env.borrow();
        memory.env = memory.create_env("env", parent_env);

        match &self.params
        {
            Either::Right(params) =>
            {
                if params.len() != args.len()
                {
                    return Err(ErrorPos{
                        position: self.body.position,
                        error: Error::WrongArgumentsCount{
                            proc: format!("<compound procedure {:?}>", self.params),
                            this_invoked: true,
                            expected: params.len().to_string(),
                            got: args.len()
                        }
                    });
                }

                params.iter().for_each(|key|
                {
                    let value = *args.pop(memory);

                    memory.define(key, value);
                });
            },
            Either::Left(name) =>
            {
                let lst = args.as_list(memory);

                memory.define(name, lst);
            }
        }

        self.body.eval(state, memory, action)
    }
}

#[derive(Debug)]
pub struct Lambdas
{
    lambdas: Vec<Rc<StoredLambda>>
}

impl Index<usize> for Lambdas
{
    type Output = Rc<StoredLambda>;

    fn index(&self, index: usize) -> &Self::Output
    {
        &self.lambdas[index]
    }
}

impl IndexMut<usize> for Lambdas
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output
    {
        &mut self.lambdas[index]
    }
}

impl Clone for Lambdas
{
    fn clone(&self) -> Self
    {
        Self{
            lambdas: clone_with_capacity(&self.lambdas)
        }
    }
}

impl Lambdas
{
    pub fn new(capacity: usize) -> Self
    {
        Self{lambdas: Vec::with_capacity(capacity)}
    }

    pub fn len(&self) -> usize
    {
        self.lambdas.len()
    }

    pub fn add(&mut self, lambda: StoredLambda) -> u32
    {
        self.add_shared(Rc::new(lambda))
    }

    pub fn add_shared(&mut self, lambda: Rc<StoredLambda>) -> u32
    {
        let id = self.lambdas.len() as u32;

        self.lambdas.push(lambda);

        id
    }

    pub fn iter(&self) -> impl Iterator<Item=&Rc<StoredLambda>>
    {
        self.lambdas.iter()
    }

    pub fn get(&self, index: u32) -> Rc<StoredLambda>
    {
        self.lambdas[index as usize].clone()
    }
    
    pub fn clear(&mut self)
    {
        self.lambdas.clear();
    }
}

#[derive(Debug, Clone)]
pub struct Primitives
{
    indices: HashMap<String, u32>,
    primitives: Vec<PrimitiveProcedureInfo>
}

impl Primitives
{
    pub fn new() -> Self
    {
        macro_rules! do_cond
        {
            ($f:expr) =>
            {
                |_state, memory, args|
                {
                    Self::do_cond(memory, args, |a, b| Some($f(a, b)), |a, b| Some($f(a, b)))
                }
            }
        }

        macro_rules! do_op
        {
            ($float_op:ident, $int_op:ident) =>
            {
                |_state, memory, args|
                {
                    Self::do_op(memory, args, |a, b|
                    {
                        Some(LispValue::new_integer(a.$int_op(b)?))
                    }, |a, b|
                    {
                        Some(LispValue::new_float(a.$float_op(b)))
                    })
                }
            }
        }

        macro_rules! do_op_simple
        {
            ($op:ident) =>
            {
                |_state, memory, args|
                {
                    Self::do_op(memory, args, |a, b|
                    {
                        Some(LispValue::new_integer(a.$op(b)))
                    }, |a, b|
                    {
                        Some(LispValue::new_float(a.$op(b)))
                    })
                }
            }
        }

        macro_rules! is_tag
        {
            ($tag:expr) =>
            {
                |_state, memory, mut args|
                {
                    let arg = args.pop(memory);

                    let is_equal = arg.tag == $tag;

                    memory.push_return(is_equal);

                    Ok(())
                }
            }
        }

        let (indices, primitives): (HashMap<_, _>, Vec<_>) = [
            ("display",
                PrimitiveProcedureInfo::new_simple_effect(1, move |_state, memory, mut args|
                {
                    let arg = args.pop(memory);

                    println!("{}", arg.to_string());

                    memory.push_return(());

                    Ok(())
                })),
            ("random-integer",
                PrimitiveProcedureInfo::new_simple(1, move |_state, memory, mut args|
                {
                    let limit = args.pop(memory).as_integer()?;

                    memory.push_return(fastrand::i32(0..limit));

                    Ok(())
                })),
            ("make-vector",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, mut args|
                {
                    let len = args.pop(memory).as_integer()? as usize;
                    let fill = args.pop(memory);

                    let vec = LispVectorRef{
                        tag: fill.tag,
                        values: &vec![(*fill).value; len]
                    };

                    memory.allocate_vector(vec);

                    Ok(())
                })),
            ("vector-set!",
                PrimitiveProcedureInfo::new_simple_effect(
                    3,
                    |_state, memory, mut args|
                    {
                        let vec = *args.pop(memory);
                        let index = *args.pop(memory);
                        let value = *args.pop(memory);

                        let vec = vec.as_vector_mut(memory.as_memory_mut())?;

                        let index = index.as_integer()?;

                        if vec.tag != value.tag
                        {
                            return Err(
                                Error::VectorWrongType{expected: vec.tag, got: value.tag}
                            );
                        }

                        *vec.values.get_mut(index as usize)
                            .ok_or_else(|| Error::IndexOutOfRange(index))? = value.value;

                        memory.push_return(());

                        Ok(())
                    })),
            ("vector-ref",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, mut args|
                {
                    let vec = *args.pop(memory);
                    let index = *args.pop(memory);

                    let vec = vec.as_vector_ref(memory.as_memory_mut())?;
                    let index = index.as_integer()?;

                    let value = *vec.values.get(index as usize)
                        .ok_or_else(|| Error::IndexOutOfRange(index))?;

                    let tag = vec.tag;
                    memory.push_return(unsafe{ LispValue::new(tag, value) });

                    Ok(())
                })),
            ("eq?", PrimitiveProcedureInfo::new_simple(2, |_state, memory, mut args|
            {
                let a = *args.pop(memory);
                let b = *args.pop(memory);

                memory.push_return(a.value == b.value);

                Ok(())
            })),
            ("null?", PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
            {
                let arg = *args.pop(memory);

                memory.push_return(arg.is_null());

                Ok(())
            })),
            ("symbol?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Symbol))),
            ("pair?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::List))),
            ("char?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Char))),
            ("vector?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Vector))),
            ("procedure?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Procedure))),
            ("number?",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);

                    let is_number = arg.tag == ValueTag::Integer || arg.tag == ValueTag::Float;

                    memory.push_return(is_number);

                    Ok(())
                })),
            ("boolean?",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);

                    let is_bool = arg.as_bool().map(|_| true).unwrap_or(false);

                    memory.push_return(is_bool);

                    Ok(())
                })),
            ("exact->inexact",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = *args.pop(memory);

                    if arg.tag == ValueTag::Float
                    {
                        memory.push_return(arg);

                        Ok(())
                    } else
                    {
                        let number = arg.as_integer()?;

                        memory.push_return(number as f32);

                        Ok(())
                    }
                })),
            ("inexact->exact",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = *args.pop(memory);

                    if arg.tag == ValueTag::Integer
                    {
                        memory.push_return(arg);

                        Ok(())
                    } else
                    {
                        let number = arg.as_float()?;

                        memory.push_return(number.round() as i32);

                        Ok(())
                    }
                })),
            ("+", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(add, checked_add))),
            ("-", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(sub, checked_sub))),
            ("*", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(mul, checked_mul))),
            ("/", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(div, checked_div))),
            ("remainder", PrimitiveProcedureInfo::new_simple(2, do_op_simple!(rem))),
            ("=",
                PrimitiveProcedureInfo::new_simple(
                    ArgsCount::Min(2),
                    do_cond!(|a, b| LispValue::new_bool(a == b)))),
            (">",
                PrimitiveProcedureInfo::new_simple(
                    ArgsCount::Min(2),
                    do_cond!(|a, b| LispValue::new_bool(a > b)))),
            ("<",
                PrimitiveProcedureInfo::new_simple(
                    ArgsCount::Min(2),
                    do_cond!(|a, b| LispValue::new_bool(a < b)))),
            ("if",
                PrimitiveProcedureInfo::new_simple_lazy(
                    2..=3,
                    Rc::new(|state, memory, args, action|
                    {
                        memory.save_env();
                        args.car().eval(state, memory, Action::Return)?;
                        memory.restore_env();

                        let predicate = memory.pop_return();

                        let on_true = args.cdr().car();
                        let on_false = args.cdr().cdr();

                        if predicate.is_true()
                        {
                            on_true.eval(state, memory, action)
                        } else
                        {
                            // this is more readable come on omg
                            #[allow(clippy::collapsible_else_if)]
                            if on_false.is_null()
                            {
                                match action
                                {
                                    Action::Return => memory.push_return(()),
                                    Action::None => ()
                                }

                                Ok(())
                            } else
                            {
                                on_false.car().eval(state, memory, action)
                            }
                        }
                    }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, args|
                {
                    let bindings = args.car();
                    let body = args.cdr().car();

                    let params = bindings.map_list(|x| x.car());
                    let apply_args = ExpressionPos::analyze_args(
                        state,
                        bindings.map_list(|x| x.cdr().car())
                    )?;

                    let lambda_args =
                        AstPos::cons(
                            params,
                            AstPos::cons(
                                body,
                                Ast::EmptyList.with_position(args.position)));

                    let lambda = ExpressionPos::analyze_lambda(state, lambda_args)?;

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(lambda),
                            args: Box::new(apply_args)
                        }
                    })
                }))),
            ("begin",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(1), Rc::new(|_op, state, args|
                {
                    ExpressionPos::analyze_sequence(state, args)
                }))),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, args|
                {
                    ExpressionPos::analyze_lambda(state, args)
                }))),
            ("define",
                PrimitiveProcedureInfo::new(ArgsCount::Min(2), Rc::new(|op, state, mut args|
                {
                    let first = args.car();

                    let is_procedure = first.is_list();

                    let args = if is_procedure
                    {
                        let position = args.position;

                        let body: Vec<_> = iter::from_fn(||
                        {
                            let next = args.cdr();
                            args = next.clone();

                            (!next.is_null()).then(|| next.car())
                        }).collect();

                        let body = if body.len() > 1
                        {
                            let body = body.into_iter().rev().fold(
                                Ast::EmptyList.with_position(position),
                                |acc, x|
                                {
                                    AstPos::cons(x, acc)
                                });

                            AstPos::cons(
                                AstPos{
                                    position: body.position,
                                    ast: Ast::Value("begin".to_owned())
                                },
                                body
                            )
                        } else
                        {
                            body.into_iter().next().unwrap()
                        };

                        let name = ExpressionPos::analyze(state, first.car())?;
                        let name = Expression::Value(name.as_value()?).with_position(position);

                        let params = first.cdr();

                        let lambda_args =
                            AstPos::cons(
                                params,
                                AstPos::cons(
                                    body,
                                    Ast::EmptyList.with_position(position)));

                        let lambda = ExpressionPos::analyze_lambda(state, lambda_args)?;

                        ExpressionPos::cons(
                            name,
                            ExpressionPos::cons(
                                lambda,
                                Expression::EmptyList.with_position(position)))
                    } else
                    {
                        ExpressionPos::analyze_args(state, args)?
                    };

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(op),
                            args: Box::new(args)
                        }
                    })
                }), Rc::new(|state, memory, args, action|
                {
                    let first = args.car();
                    let second = args.cdr().car();

                    let key = first.as_value()?;

                    memory.save_env();
                    second.eval(state, memory, Action::Return)?;
                    memory.restore_env();

                    let value = memory.pop_return();

                    memory.define(key, value);

                    if action == Action::Return
                    {
                        memory.push_return(());
                    }

                    Ok(())
                }))),
            ("quote",
                PrimitiveProcedureInfo::new(ArgsCount::Min(0), Rc::new(|op, state, args|
                {
                    let arg = ExpressionPos::quote(state, args.car())?;

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(op),
                            args: Box::new(arg)
                        }
                    })
                }), Rc::new(|_state, memory, args, action|
                {
                    if let Action::None = action
                    {
                        return Ok(());
                    }

                    memory.allocate_expression(args);

                    Ok(())
                }))),
            ("cons",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, _args|
                {
                    // yea yea its the reverse version, i just push the args from back to front
                    memory.rcons();

                    Ok(())
                })),
            ("car",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);
                    let value = *arg.as_list()?.car;

                    memory.push_return(value);

                    Ok(())
                })),
            ("cdr",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);
                    let value = *arg.as_list()?.cdr;

                    memory.push_return(value);

                    Ok(())
                })),
            ("set-car!",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);
                    let list_id = arg.as_list_id()?;

                    let value = *args.pop(memory);

                    memory.set_car(list_id, value);

                    memory.push_return(());

                    Ok(())
                })),
            ("set-cdr!",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, mut args|
                {
                    let arg = args.pop(memory);
                    let list_id = arg.as_list_id()?;

                    let value = *args.pop(memory);

                    memory.set_cdr(list_id, value);

                    memory.push_return(());

                    Ok(())
                }))
        ].into_iter().enumerate().map(|(index, (k, v))|
        {
            ((k.to_owned(), index as u32), v)
        }).unzip();

        Self{
            indices,
            primitives
        }
    }

    pub fn add(&mut self, name: impl Into<String>, procedure: PrimitiveProcedureInfo)
    {
        let name = name.into();

        let id = self.primitives.len();

        self.primitives.push(procedure);
        self.indices.insert(name, id as u32);
    }

    pub fn name_by_index(&self, index: u32) -> &str
    {
        self.indices.iter().find(|(_key, value)|
        {
            **value == index
        }).expect("index must exist").0
    }

    pub fn index_by_name(&self, name: &str) -> Option<u32>
    {
        self.indices.get(name).copied()
    }

    pub fn get_by_name(&self, name: &str) -> Option<&PrimitiveProcedureInfo>
    {
        self.index_by_name(name).map(|index| self.get(index))
    }

    pub fn get(&self, id: u32) -> &PrimitiveProcedureInfo
    {
        &self.primitives[id as usize]
    }

    fn call_op<FI, FF>(
        a: LispValue,
        b: LispValue,
        op_integer: FI,
        op_float: FF
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> Option<LispValue>,
        FF: Fn(f32, f32) -> Option<LispValue>
    {
        // i cant be bothered with implicit type coercions im just gonna error

        macro_rules! number_error
        {
            ($a:expr, $b:expr) =>
            {
                Error::OperationError{a: $a.to_string(), b: $b.to_string()}
            }
        }

        match (a.tag(), b.tag())
        {
            (ValueTag::Integer, ValueTag::Integer) =>
            {
                let (a, b) = (a.as_integer().unwrap(), b.as_integer().unwrap());

                op_integer(a, b).ok_or_else(||
                {
                    number_error!(a, b)
                })
            },
            (ValueTag::Float, ValueTag::Float) =>
            {
                let (a, b) = (a.as_float().unwrap(), b.as_float().unwrap());

                op_float(a, b).ok_or_else(||
                {
                    number_error!(a, b)
                })
            },
            (ValueTag::Float, ValueTag::Integer)
            | (ValueTag::Integer, ValueTag::Float) =>
            {
                let (a, b) = if a.tag() == ValueTag::Float
                {
                    (a.as_float().unwrap(), b.as_integer().unwrap() as f32)
                } else
                {
                    (a.as_integer().unwrap() as f32, b.as_float().unwrap())
                };

                op_float(a, b).ok_or_else(||
                {
                    number_error!(a, b)
                })
            },
            (a, b) => Err(Error::ExpectedNumerical{a, b})
        }
    }

    fn do_cond<FI, FF>(
        memory: &mut LispMemory,
        mut args: ArgsWrapper,
        op_integer: FI,
        op_float: FF
    ) -> Result<(), Error>
    where
        FI: Fn(i32, i32) -> Option<LispValue>,
        FF: Fn(f32, f32) -> Option<LispValue>
    {
        let first = *args.pop(memory);
        let second = *args.pop(memory);

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        let is_true = output.as_bool()?;

        if !is_true || args.is_empty()
        {
            args.clear(memory);

            memory.push_return(output);

            Ok(())
        } else
        {
            args.push(memory, second);

            Self::do_cond(memory, args, op_integer, op_float)
        }
    }

    fn do_op<FI, FF>(
        memory: &mut LispMemory,
        mut args: ArgsWrapper,
        op_integer: FI,
        op_float: FF
    ) -> Result<(), Error>
    where
        FI: Fn(i32, i32) -> Option<LispValue>,
        FF: Fn(f32, f32) -> Option<LispValue>
    {
        let first = *args.pop(memory);
        let second = *args.pop(memory);

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        if args.is_empty()
        {
            memory.push_return(output);

            Ok(())
        } else
        {
            args.push(memory, output);

            Self::do_op(memory, args, op_integer, op_float)
        }
    }
}

#[derive(Debug, Clone)]
pub struct State
{
    pub primitives: Rc<Primitives>
}

#[derive(Debug, Clone)]
pub struct Program
{
    state: State,
    expression: ExpressionPos
}

impl Program
{
    pub fn parse(
        primitives: Rc<Primitives>,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        let ast = Parser::parse(code)?;

        let mut state = State{
            primitives
        };

        let expression = ExpressionPos::analyze_sequence(&mut state, ast)?;

        Ok(Self{state, expression})
    }

    pub fn eval(
        &self,
        memory: &mut LispMemory
    ) -> Result<LispValue, ErrorPos>
    {
        self.expression.eval(&self.state, memory, Action::Return)?;

        Ok(memory.pop_return())
    }
}

#[derive(Debug, Clone)]
pub struct ArgValues<T=LispValue>
{
    position: CodePosition,
    args: Vec<T>
}

impl<T> ArgValues<T>
{
    pub fn new(position: CodePosition) -> Self
    {
        Self{position, args: Vec::new()}
    }

    pub fn position(&self) -> CodePosition
    {
        self.position
    }

    pub fn is_empty(&self) -> bool
    {
        self.args.is_empty()
    }

    pub fn len(&self) -> usize
    {
        self.args.len()
    }

    pub fn iter(&self) -> impl Iterator<Item=&T>
    {
        self.args.iter().rev()
    }

    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item=&mut T>
    {
        self.args.iter_mut().rev()
    }

    pub fn pop(&mut self) -> Result<T, ErrorPos>
    {
        let top = self.args.pop();

        top.ok_or(ErrorPos{position: self.position, error: Error::ExpectedArg})
    }

    pub fn push(&mut self, value: T)
    {
        self.args.push(value);
    }
}

impl<T> IntoIterator for ArgValues<T>
{
    type Item = T;
    type IntoIter = iter::Rev<vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter
    {
        self.args.into_iter().rev()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action
{
    None,
    Return
}

#[derive(Clone)]
pub struct ExpressionPos
{
    pub position: CodePosition,
    pub expression: Expression
}

impl Debug for ExpressionPos
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{} {:#?}", self.position, self.expression)
    }
}

impl ExpressionPos
{
    pub fn cons(car: Self, cdr: Self) -> Self
    {
        Self{
            position: car.position,
            expression: Expression::List{car: Box::new(car), cdr: Box::new(cdr)}
        }
    }

    pub fn as_value(&self) -> Result<String, ErrorPos>
    {
        match &self.expression
        {
            Expression::Value(x) => Ok(x.clone()),
            _ => Err(ErrorPos{position: self.position, error: Error::ExpectedOp})
        }
    }

    pub fn eval(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        action: Action
    ) -> Result<(), ErrorPos>
    {
        let value = match &self.expression
        {
            Expression::Integer(x) =>
            {
                LispValue::new_integer(*x)
            },
            Expression::Float(x) =>
            {
                LispValue::new_float(*x)
            },
            Expression::Bool(x) =>
            {
                LispValue::new_bool(*x)
            },
            Expression::PrimitiveProcedure(id) =>
            {
                LispValue::new_primitive_procedure(*id)
            },
            Expression::Value(s) =>
            {
                memory.lookup(s)
                    .ok_or_else(|| Error::UndefinedVariable(s.to_owned()))
                    .with_position(self.position)?
            },
            Expression::Lambda{body, params} =>
            {
                let lambda = StoredLambda::new(memory, (**params).clone(), (**body).clone())?;

                let id = memory.lambdas_mut().add(lambda);

                LispValue::new_procedure(id)
            },
            Expression::Application{op, args} =>
            {
                return op.apply(state, memory, args, action);
            },
            Expression::Sequence{first, after} =>
            {
                memory.save_env();
                first.eval(state, memory, Action::None)?;
                memory.restore_env();

                return after.eval(state, memory, action);
            },
            _ => return Err(ErrorPos{position: self.position, error: Error::ApplyNonApplication})
        };

        match action
        {
            Action::Return =>
            {
                memory.push_return(value);

                Ok(())
            },
            Action::None => Ok(())
        }
    }

    pub fn apply(
        &self,
        state: &State,
        memory: &mut LispMemory,
        args: &Self,
        action: Action
    ) -> Result<(), ErrorPos>
    {
        memory.save_env();
        self.eval(state, memory, Action::Return)?;
        memory.restore_env();

        let op = memory.pop_return();

        if let Ok(op) = op.as_primitive_procedure()
        {
            let primitive = state.primitives.get(op);

            let got = Expression::arg_count(args);
            let correct = match primitive.args_count
            {
                ArgsCount::Some(count) => got == count,
                ArgsCount::Between{start, end_inclusive} =>
                {
                    (start..=end_inclusive).contains(&got)
                },
                ArgsCount::Min(expected) =>
                {
                    got >= expected
                }
            };

            if !correct
            {
                let error = Error::WrongArgumentsCount{
                    proc: state.primitives.name_by_index(op).to_owned(),
                    this_invoked: true,
                    expected: primitive.args_count.to_string(),
                    got
                };

                return Err(ErrorPos{
                    position: args.position,
                    error
                });
            }

            if crate::DEBUG_LISP
            {
                eprintln!(
                    "({}) called primitive: {}",
                    self.position,
                    state.primitives.name_by_index(op)
                );
            }

            let returns_len = memory.returns_len();
            (primitive.on_apply.as_ref().expect("must have apply"))(
                state,
                memory,
                args,
                action
            )?;

            if action == Action::Return
            {
                debug_assert!(
                    (returns_len + 1) == memory.returns_len(),
                    "primitive procedure must return 1 value (action return)"
                );
            } else
            {
                debug_assert!(
                    returns_len == memory.returns_len(),
                    "primitive procedure must return no value (action none)"
                );
            }

            Ok(())
        } else
        {
            memory.push_op(op);

            let args = args.eval_args(state, memory, Action::Return)?;

            let op = memory.pop_op();
            let op = op.as_procedure().with_position(self.position)?;

            let proc = memory.lambdas().get(op);

            proc.apply(state, memory, args, action).map_err(|mut err|
            {
                #[allow(clippy::single_match)]
                match &mut err.error
                {
                    Error::WrongArgumentsCount{this_invoked, ..} =>
                    {
                        if *this_invoked
                        {
                            err.position = self.position;

                            *this_invoked = false;
                        }
                    },
                    _ => ()
                }

                err
            })
        }
    }

    pub fn map_list<T, F>(&self, mut f: F) -> Result<ArgValues<T>, ErrorPos>
    where
        F: FnMut(&Self) -> Result<T, ErrorPos>
    {
        if self.is_null()
        {
            // i got it backwards, whatever who CARES
            Ok(ArgValues::new(self.position))
        } else
        {
            let car = f(self.car())?;
            let cdr = self.cdr();

            let mut args = cdr.map_list(f)?;

            args.push(car);

            Ok(args)
        }
    }

    pub fn eval_args(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        action: Action
    ) -> Result<ArgsWrapper, ErrorPos>
    {
        self.eval_args_inner(state, memory, action, false)
    }

    fn eval_args_inner(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        action: Action,
        save_env: bool
    ) -> Result<ArgsWrapper, ErrorPos>
    {
        if self.is_null()
        {
            Ok(ArgsWrapper::new())
        } else
        {
            let car = self.car();
            let cdr = self.cdr();

            let args = cdr.eval_args_inner(state, memory, action, true)?;

            if save_env
            {
                memory.save_env();
            }

            car.eval(state, memory, action)?;

            if save_env
            {
                memory.restore_env();
            }

            Ok(args.increment())
        }
    }

    pub fn quote(state: &mut State, ast: AstPos) -> Result<Self, ErrorPos>
    {
        let expression = if ast.is_list()
        {
            if ast.is_null()
            {
                Expression::EmptyList
            } else
            {
                let car = Self::quote(state, ast.car())?;
                let cdr = Self::quote(state, ast.cdr())?;

                Expression::List{
                    car: Box::new(car),
                    cdr: Box::new(cdr)
                }
            }
        } else
        {
            Expression::analyze_primitive_ast(ast.as_value()?)
        };

        Ok(Self{position: ast.position, expression})
    }

    pub fn analyze(state: &mut State, ast: AstPos) -> Result<Self, ErrorPos>
    {
        if ast.is_list()
        {
            let op = Self::analyze(state, ast.car())?;

            let args = ast.cdr();
            Self::analyze_op(state, op, args)
        } else
        {
            Self::analyze_atom(state, ast)
        }
    }

    pub fn analyze_op(
        state: &mut State,
        op: Self,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if let Expression::PrimitiveProcedure(id) = op.expression
        {
            if let Some(on_eval) = state.primitives.get(id).on_eval.as_ref().map(|x| x.clone())
            {
                return on_eval(op, state, ast);
            }
        }

        Ok(Self{
            position: op.position,
            expression: Expression::Application{
                op: Box::new(op),
                args: Box::new(Self::analyze_args(state, ast)?)
            }
        })
    }

    pub fn analyze_lambda(
        state: &mut State,
        args: AstPos
    ) -> Result<Self, ErrorPos>
    {
        Expression::argument_count_ast("lambda".to_owned(), 2, &args)?;

        let params = Box::new(ExpressionPos::analyze_params(state, args.car())?);
        let body = Box::new(Self::analyze(state, args.cdr().car())?);

        Ok(Self{position: args.position, expression: Expression::Lambda{body, params}})
    }

    pub fn analyze_params(
        state: &mut State,
        params: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if params.is_list()
        {
            if params.is_null()
            {
                return Ok(Self{position: params.position, expression: Expression::EmptyList});
            }

            let out = Expression::List{
                car: Box::new(Self::analyze_param(state, params.car())?),
                cdr: Box::new(Self::analyze_params(state, params.cdr())?)
            };

            Ok(Self{position: params.position, expression: out})
        } else
        {
            Self::analyze_param(state, params)
        }
    }

    pub fn analyze_param(
        state: &mut State,
        param: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let expression = match param.ast
        {
            Ast::Value(x) =>
            {
                Self::check_shadowing(state, &x).with_position(param.position)?;

                Expression::Value(x)
            },
            _ => return Err(ErrorPos{position: param.position, error: Error::ExpectedParam})
        };

        Ok(Self{position: param.position, expression})
    }

    fn check_shadowing(
        state: &mut State,
        name: &str
    ) -> Result<(), Error>
    {
        if state.primitives.get_by_name(name).is_some()
        {
            return Err(Error::AttemptedShadowing(name.to_owned()));
        }

        Ok(())
    }

    pub fn analyze_args(
        state: &mut State,
        args: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if args.is_null()
        {
            return Ok(Self{position: args.position, expression: Expression::EmptyList});
        }

        let out = Expression::List{
            car: Box::new(Self::analyze(state, args.car())?),
            cdr: Box::new(Self::analyze_args(state, args.cdr())?)
        };

        Ok(Self{position: args.position, expression: out})
    }

    pub fn analyze_atom(
        state: &mut State,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let expression = Expression::analyze_primitive_ast(ast.as_value()?);

        Ok(Self{
            position: ast.position,
            expression: match expression
            {
                Expression::Value(ref x) =>
                {
                    if let Some(id) = state.primitives.index_by_name(x)
                    {
                        Expression::PrimitiveProcedure(id)
                    } else
                    {
                        expression
                    }
                },
                x => x
            }
        })
    }

    pub fn analyze_sequence(
        state: &mut State,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if ast.is_null()
        {
            return Err(ErrorPos{position: ast.position, error: Error::EmptySequence});
        }

        let car = Self::analyze(state, ast.car())?;
        let cdr = ast.cdr();

        Ok(if cdr.is_null()
        {
            car
        } else
        {
            Self{
                position: car.position,
                expression: Expression::Sequence{
                    first: Box::new(car),
                    after: Box::new(Self::analyze_sequence(state, cdr)?)
                }
            }
        })
    }

    pub fn argument_count(name: String, count: usize, args: &Self) -> Result<(), ErrorPos>
    {
        Expression::argument_count_inner(name, args.position, count, Expression::arg_count(args))
    }
}

impl Deref for ExpressionPos
{
    type Target = Expression;

    fn deref(&self) -> &Self::Target
    {
        &self.expression
    }
}

#[derive(Debug, Clone)]
pub enum Expression
{
    Value(String),
    PrimitiveProcedure(u32),
    Float(f32),
    Integer(i32),
    Bool(bool),
    EmptyList,
    Lambda{body: Box<ExpressionPos>, params: Box<ExpressionPos>},
    List{car: Box<ExpressionPos>, cdr: Box<ExpressionPos>},
    Application{op: Box<ExpressionPos>, args: Box<ExpressionPos>},
    Sequence{first: Box<ExpressionPos>, after: Box<ExpressionPos>}
}

impl Expression
{
    pub fn car(&self) -> &ExpressionPos
    {
        match self
        {
            Self::List{car, ..} => car,
            x => panic!("car must be called on a list, called on {x:?}")
        }
    }

    pub fn cdr(&self) -> &ExpressionPos
    {
        match self
        {
            Self::List{cdr, ..} => cdr,
            x => panic!("cdr must be called on a list, called on {x:?}")
        }
    }

    pub fn is_null(&self) -> bool
    {
        match self
        {
            Self::EmptyList => true,
            _ => false
        }
    }

    pub fn is_list(&self) -> bool
    {
        match self
        {
            Self::List{..} | Self::EmptyList => true,
            _ => false
        }
    }

    pub fn analyze_primitive_ast(
        primitive: PrimitiveType
    ) -> Self
    {
        match primitive
        {
            PrimitiveType::Value(x) => Self::Value(x),
            PrimitiveType::Float(x) => Self::Float(x),
            PrimitiveType::Integer(x) => Self::Integer(x),
            PrimitiveType::Bool(x) => Self::Bool(x)
        }
    }

    pub fn argument_count_ast(name: String, count: usize, args: &AstPos) -> Result<(), ErrorPos>
    {
        Self::argument_count_inner(name, args.position, count, Self::arg_count_ast(args))
    }

    fn argument_count_inner(
        name: String,
        position: CodePosition,
        expected: usize,
        got: usize
    ) -> Result<(), ErrorPos>
    {
        if got == expected
        {
            Ok(())
        } else
        {
            Err(ErrorPos{
                position,
                error: Error::WrongArgumentsCount{
                    proc: name,
                    this_invoked: true,
                    expected: expected.to_string(),
                    got
                }
            })
        }
    }

    fn arg_count(args: &Self) -> usize
    {
        if !args.is_list()
        {
            1
        } else if args.is_null()
        {
            0
        } else
        {
            1 + Self::arg_count(args.cdr())
        }
    }

    fn arg_count_ast(args: &AstPos) -> usize
    {
        if args.is_null() || !args.is_list()
        {
            0
        } else
        {
            1 + Self::arg_count_ast(&args.cdr())
        }
    }
}
