use std::{
    vec,
    iter,
    rc::Rc,
    cell::RefCell,
    fmt::{self, Debug},
    collections::HashMap,
    ops::{RangeInclusive, Add, Sub, Mul, Div, Rem, Deref}
};

pub use super::{
    Error,
    ErrorPos,
    Environment,
    LispValue,
    LispMemory,
    ValueTag,
    LispVectorRef
};

pub use parser::{CodePosition, WithPosition};

use parser::{Parser, Ast, AstPos, PrimitiveType};

mod parser;


// unreadable, great
pub type OnApply = Rc<
    dyn Fn(
        &State,
        &mut LispMemory,
        &Rc<Environment>,
        &ExpressionPos,
        Action
    ) -> Result<(), ErrorPos>>;

pub type OnEval = Rc<
    dyn Fn(
        ExpressionPos,
        &mut State,
        &Rc<Environment>,
        AstPos
    ) -> Result<ExpressionPos, ErrorPos>>;

pub fn simple_apply<const EFFECT: bool>(f: impl Fn(
    &State,
    &mut LispMemory,
    &Rc<Environment>,
    ArgsWrapper
) -> Result<LispValue, Error> + 'static) -> OnApply
{
    Rc::new(move |
        state: &State,
        memory: &mut LispMemory,
        env: &Rc<Environment>,
        args: &ExpressionPos,
        action: Action
    |
    {
        let value = {
            let position = args.position;

            let action = if EFFECT { Action::Return } else { action };
            let args = args.apply_args(state, memory, env, action)?;

            if !EFFECT
            {
                match action
                {
                    Action::Return => (),
                    Action::None => return Ok(())
                }
            }

            f(state, memory, env, args).with_position(position)
        }?;

        match action
        {
            Action::Return =>
            {
                memory.push_return(value);
            },
            Action::None => ()
        }

        Ok(())
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

    pub fn pop(&mut self, memory: &mut LispMemory) -> LispValue
    {
        self.try_pop(memory).expect("pop must be called on argcount > 0")
    }

    pub fn try_pop(&mut self, memory: &mut LispMemory) -> Option<LispValue>
    {
        if self.count == 0
        {
            return None;
        }

        self.count -= 1;

        Some(memory.pop_return())
    }

    pub fn push(&mut self, memory: &mut LispMemory, value: LispValue)
    {
        self.count += 1;

        memory.push_return(value);
    }

    pub fn as_list(&mut self, env: &Environment, memory: &mut LispMemory) -> LispValue
    {
        let lst = (0..self.count).fold(LispValue::new_empty_list(), |acc, _|
        {
            let value = memory.pop_return();
            memory.cons(env, value, acc)
        });

        self.count = 0;

        lst
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
    Some(usize),
    None
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

impl From<Option<usize>> for ArgsCount
{
    fn from(value: Option<usize>) -> Self
    {
        match value
        {
            Some(x) => Self::Some(x),
            None => Self::None
        }
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
            &mut LispMemory,
            &Rc<Environment>,
            ArgsWrapper
        ) -> Result<LispValue, Error> + 'static
    {
        Self::new_simple_maybe_effect::<true, F>(args_count, on_apply)
    }

    pub fn new_simple<F>(
        args_count: impl Into<ArgsCount>,
        on_apply: F
    ) -> Self
    where
        F: Fn(
            &State,
            &mut LispMemory,
            &Rc<Environment>,
            ArgsWrapper
        ) -> Result<LispValue, Error> + 'static
    {
        Self::new_simple_maybe_effect::<false, F>(args_count, on_apply)
    }

    fn new_simple_maybe_effect<const EFFECT: bool, F>(
        args_count: impl Into<ArgsCount>,
        on_apply: F
    ) -> Self
    where
        F: Fn(
            &State,
            &mut LispMemory,
            &Rc<Environment>,
            ArgsWrapper
        ) -> Result<LispValue, Error> + 'static
    {
        let on_apply = simple_apply::<EFFECT>(on_apply);

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
        let args_count = match self.args_count
        {
            ArgsCount::Some(x) => x.to_string(),
            ArgsCount::None => "no".to_owned(),
            ArgsCount::Between{start, end_inclusive} => format!("between {start} and {end_inclusive}"),
            ArgsCount::Min(x) => format!("at least {x}")
        };

        write!(f, "<procedure with {args_count} args>")
    }
}

#[derive(Clone)]
pub struct PrimitiveProcedure(pub OnApply);

impl Debug for PrimitiveProcedure
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "<primitive procedure>")
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
    env: Rc<Environment>,
    params: Either<String, ArgValues<String>>,
    body: ExpressionPos
}

impl StoredLambda
{
    pub fn new(
        env: Rc<Environment>,
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

        Ok(Self{env, params, body})
    }

    pub fn apply(
        &self,
        state: &State,
        memory: &mut LispMemory,
        mut args: ArgsWrapper,
        action: Action
    ) -> Result<(), ErrorPos>
    {
        let new_env = Rc::new(Environment::child(self.env.clone()));

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
                            expected: params.len(),
                            got: args.len()
                        }
                    });
                }

                params.iter().for_each(|key|
                {
                    let value = args.pop(memory);

                    new_env.define(key, value);
                });
            },
            Either::Left(name) =>
            {
                let lst = args.as_list(&self.env, memory);

                new_env.define(name, lst);
            }
        }

        self.body.apply(state, memory, &new_env, action)
    }
}

#[derive(Debug, Clone)]
pub struct Lambdas
{
    lambdas: Vec<Rc<RefCell<StoredLambda>>>
}

impl Lambdas
{
    pub fn new() -> Self
    {
        Self{lambdas: Vec::new()}
    }

    pub fn add(&mut self, lambda: StoredLambda) -> usize
    {
        let id = self.lambdas.len();

        self.lambdas.push(Rc::new(RefCell::new(lambda)));

        id
    }

    pub fn get(&self, index: usize) -> Rc<RefCell<StoredLambda>>
    {
        self.lambdas[index].clone()
    }
    
    pub fn clear(&mut self)
    {
        self.lambdas.clear();
    }
}

#[derive(Debug, Clone)]
pub struct Primitives
{
    indices: HashMap<String, usize>,
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
                |_state, memory, _env, args|
                {
                    Self::do_cond(memory, args, $f, $f)
                }
            }
        }

        macro_rules! do_op
        {
            ($op:ident) =>
            {
                |_state, memory, _env, args|
                {
                    Self::do_op(memory, args, |a, b|
                    {
                        LispValue::new_integer(a.$op(b))
                    }, |a, b|
                    {
                        LispValue::new_float(a.$op(b))
                    })
                }
            }
        }

        macro_rules! is_tag
        {
            ($tag:expr) =>
            {
                |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);

                    let is_equal = arg.tag == $tag;

                    Ok(LispValue::new_bool(is_equal))
                }
            }
        }

        let (indices, primitives): (HashMap<_, _>, Vec<_>) = [
            ("display",
                PrimitiveProcedureInfo::new_simple_effect(1, move |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);

                    println!("{}", arg.to_string(memory));

                    Ok(LispValue::new_empty_list())
                })),
            ("random-integer",
                PrimitiveProcedureInfo::new_simple(1, move |_state, memory, _env, mut args|
                {
                    let limit = args.pop(memory).as_integer()?;

                    Ok(LispValue::new_integer(fastrand::i32(0..limit)))
                })),
            ("make-vector",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, env, mut args|
                {
                    let len = args.pop(memory).as_integer()? as usize;
                    let fill = args.pop(memory);

                    let vec = LispVectorRef{
                        tag: fill.tag,
                        values: &vec![fill.value; len]
                    };

                    Ok(LispValue::new_vector(memory.allocate_vector(env, vec)))
                })),
            ("vector-set!",
                PrimitiveProcedureInfo::new_simple_effect(
                    3,
                    |_state, memory, _env, mut args|
                    {
                        let vec = args.pop(memory);
                        let index = args.pop(memory);
                        let value = args.pop(memory);

                        let vec = vec.as_vector_mut(memory)?;

                        let index = index.as_integer()?;

                        if vec.tag != value.tag
                        {
                            return Err(
                                Error::VectorWrongType{expected: vec.tag, got: value.tag}
                            );
                        }

                        Self::check_inbounds(vec.values, index)?;

                        vec.values[index as usize] = value.value;

                        Ok(LispValue::new_empty_list())
                    })),
            ("vector-ref",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, _env, mut args|
                {
                    let vec = args.pop(memory);
                    let index = args.pop(memory);

                    let vec = vec.as_vector_ref(memory)?;
                    let index = index.as_integer()?;

                    Self::check_inbounds(vec.values, index)?;

                    let value = vec.values[index as usize];

                    Ok(unsafe{ LispValue::new(vec.tag, value) })
                })),
            ("null?", PrimitiveProcedureInfo::new_simple(1, |_state, memory, _env, mut args|
            {
                let arg = args.pop(memory);

                Ok(LispValue::new_bool(arg.is_null()))
            })),
            ("symbol?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Symbol))),
            ("pair?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::List))),
            ("char?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Char))),
            ("vector?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Vector))),
            ("procedure?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Procedure))),
            ("number?",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);

                    let is_number = arg.tag == ValueTag::Integer || arg.tag == ValueTag::Float;

                    Ok(LispValue::new_bool(is_number))
                })),
            ("boolean?",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);

                    let is_bool = arg.as_bool().map(|_| true).unwrap_or(false);

                    Ok(LispValue::new_bool(is_bool))
                })),
            ("+", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(add))),
            ("-", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(sub))),
            ("*", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(mul))),
            ("/", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), do_op!(div))),
            ("remainder", PrimitiveProcedureInfo::new_simple(2, do_op!(rem))),
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
                    Rc::new(|state, memory, env, args, action|
                    {
                        args.car().apply(state, memory, env, Action::Return)?;
                        let predicate = memory.pop_return();

                        let on_true = args.cdr().car();
                        let on_false = args.cdr().cdr();

                        if predicate.is_true()
                        {
                            on_true.apply(state, memory, env, action)
                        } else
                        {
                            // this is more readable come on omg
                            #[allow(clippy::collapsible_else_if)]
                            if on_false.is_null()
                            {
                                match action
                                {
                                    Action::Return => memory.push_return(LispValue::new_empty_list()),
                                    Action::None => ()
                                }

                                Ok(())
                            } else
                            {
                                on_false.car().apply(state, memory, env, action)
                            }
                        }
                    }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, env, args|
                {
                    let bindings = args.car();
                    let body = args.cdr().car();

                    let params = bindings.map_list(|x| x.car());
                    let apply_args = ExpressionPos::eval_args(
                        state,
                        env,
                        bindings.map_list(|x| x.cdr().car())
                    )?;

                    let lambda_args =
                        AstPos::cons(
                            params,
                            AstPos::cons(
                                body,
                                Ast::EmptyList.with_position(args.position)));

                    let lambda = ExpressionPos::eval_lambda(state, env, lambda_args)?;

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(lambda),
                            args: Box::new(apply_args)
                        }
                    })
                }))),
            ("begin",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(1), Rc::new(|_op, state, env, args|
                {
                    ExpressionPos::eval_sequence(state, env, args)
                }))),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, env, args|
                {
                    ExpressionPos::eval_lambda(state, env, args)
                }))),
            ("define",
                PrimitiveProcedureInfo::new(ArgsCount::Min(2), Rc::new(|op, state, env, mut args|
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

                        let name = Expression::ast_to_expression(first.car())?;
                        let name = Expression::Value(name.as_value()?).with_position(position);

                        let params = first.cdr();

                        let lambda_args =
                            AstPos::cons(
                                params,
                                AstPos::cons(
                                    body,
                                    Ast::EmptyList.with_position(position)));

                        let lambda = ExpressionPos::eval_lambda(state, env, lambda_args)?;

                        ExpressionPos::cons(
                            name,
                            ExpressionPos::cons(
                                lambda,
                                Expression::EmptyList.with_position(position)))
                    } else
                    {
                        ExpressionPos::eval_args(state, env, args)?
                    };

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(op),
                            args: Box::new(args)
                        }
                    })
                }), Rc::new(|state, memory, env, args, action|
                {
                    let first = args.car();
                    let second = args.cdr().car();

                    let key = first.as_value()?;

                    second.apply(state, memory, env, Action::Return)?;
                    let value = memory.pop_return();

                    env.define(key, value);

                    match action
                    {
                        Action::Return => memory.push_return(LispValue::new_empty_list()),
                        Action::None => ()
                    }

                    Ok(())
                }))),
            ("quote",
                PrimitiveProcedureInfo::new(1, Rc::new(|op, _state, _env, args|
                {
                    let arg = Expression::ast_to_expression(args.car())?;

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(op),
                            args: Box::new(arg)
                        }
                    })
                }), Rc::new(|_state, memory, env, args, action|
                {
                    match action
                    {
                        Action::Return => (),
                        Action::None => return Ok(())
                    }

                    let value = memory.allocate_expression(env, args);
                    memory.push_return(value);

                    Ok(())
                }))),
            ("cons",
                PrimitiveProcedureInfo::new_simple(2, |_state, memory, env, mut args|
                {
                    let car = args.pop(memory);
                    let cdr = args.pop(memory);

                    Ok(memory.cons(env, car, cdr))
                })),
            ("car",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);
                    let value = arg.as_list(memory)?;

                    Ok(value.car)
                })),
            ("cdr",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, _env, mut args|
                {
                    let arg = args.pop(memory);
                    let value = arg.as_list(memory)?;

                    Ok(value.cdr)
                })),
        ].into_iter().enumerate().map(|(index, (k, v))|
        {
            ((k.to_owned(), index), v)
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
        self.indices.insert(name, id);
    }

    pub fn get_by_name(&self, name: &str) -> Option<&PrimitiveProcedureInfo>
    {
        self.indices.get(name).map(|index| self.get(*index))
    }

    pub fn get(&self, id: usize) -> &PrimitiveProcedureInfo
    {
        &self.primitives[id]
    }

    pub fn add_to_env(&self, env: &Rc<Environment>)
    {
        self.indices.iter()
            .for_each(|(key, value)|
            {
                let value = LispValue::new_primitive_procedure(*value);

                env.define(key, value);
            });
    }

    fn call_op<FI, FF>(
        a: LispValue,
        b: LispValue,
        op_integer: FI,
        op_float: FF
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> LispValue,
        FF: Fn(f32, f32) -> LispValue
    {
        // i cant be bothered with implicit type coercions im just gonna panic

        let output = match (a.tag(), b.tag())
        {
            (ValueTag::Integer, ValueTag::Integer) =>
            {
                op_integer(a.as_integer()?, b.as_integer()?)
            },
            (ValueTag::Float, ValueTag::Float) =>
            {
                op_float(a.as_float()?, b.as_float()?)
            },
            _ => return Err(Error::ExpectedSameNumberType)
        };

        Ok(output)
    }

    fn do_cond<FI, FF>(
        memory: &mut LispMemory,
        mut args: ArgsWrapper,
        op_integer: FI,
        op_float: FF
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> LispValue,
        FF: Fn(f32, f32) -> LispValue
    {
        let first = args.pop(memory);
        let second = args.pop(memory);

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        let is_true = output.as_bool()?;

        if !is_true || args.is_empty()
        {
            args.clear(memory);

            Ok(output)
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
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> LispValue,
        FF: Fn(f32, f32) -> LispValue
    {
        let first = args.pop(memory);
        let second = args.pop(memory);

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        if args.is_empty()
        {
            Ok(output)
        } else
        {
            args.push(memory, output);

            Self::do_op(memory, args, op_integer, op_float)
        }
    }

    fn check_inbounds<T>(values: &[T], index: i32) -> Result<(), Error>
    {
        if index < 0 || index as usize >= values.len()
        {
            Err(Error::IndexOutOfRange(index))
        } else
        {
            Ok(())
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
    env: Rc<Environment>,
    default_lambdas: Option<Lambdas>,
    state: State,
    expression: ExpressionPos
}

impl Program
{
    pub fn parse(
        env: Rc<Environment>,
        primitives: Rc<Primitives>,
        lambdas: Option<Lambdas>,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        primitives.add_to_env(&env);

        let ast = Parser::parse(code)?;

        let mut state = State{
            primitives
        };

        let expression = ExpressionPos::eval_sequence(&mut state, &env, ast)?;

        Ok(Self{env, default_lambdas: lambdas, state, expression})
    }

    pub fn lambdas(&self) -> &Lambdas
    {
        todo!()
    }

    pub fn apply(
        &self,
        memory: &mut LispMemory
    ) -> Result<(Rc<Environment>, LispValue), ErrorPos>
    {
        let env = {
            let env: &Environment = &self.env;

            Rc::new(env.clone())
        };

        self.expression.apply(&self.state, memory, &env, Action::Return)?;
        memory.lambdas_mut().clear();

        Ok((env, memory.pop_return()))
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

#[derive(Debug, Clone, Copy)]
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
        write!(f, "{:#?}", self.expression)
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

    pub fn apply(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        env: &Rc<Environment>,
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
            Expression::Value(s) =>
            {
                env.lookup(s).map_err(|error| ErrorPos{position: self.position, error})?
            },
            Expression::Lambda{body, params} =>
            {
                let lambda = StoredLambda::new(env.clone(), (**params).clone(), (**body).clone())?;

                let id = memory.lambdas_mut().add(lambda);

                LispValue::new_procedure(id)
            },
            Expression::Application{op, args} =>
            {
                return op.apply_application(state, memory, env, args, action);
            },
            Expression::Sequence{first, after} =>
            {
                first.apply(state, memory, env, Action::None)?;

                return after.apply(state, memory, env, action);
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

    pub fn apply_application(
        &self,
        state: &State,
        memory: &mut LispMemory,
        env: &Rc<Environment>,
        args: &Box<Self>,
        action: Action
    ) -> Result<(), ErrorPos>
    {
        self.apply(state, memory, env, Action::Return)?;
        let op = memory.pop_return();

        if let Ok(op) = op.as_primitive_procedure()
        {
            (state.primitives.get(op).on_apply.as_ref().expect("must have apply"))(
                state,
                memory,
                env,
                args,
                action
            )
        } else
        {
            let op = op.as_procedure().with_position(self.position)?;
            let args = args.apply_args(state, memory, env, Action::Return)?;

            let proc = memory.lambdas().get(op);
            let proc = proc.borrow();

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

    pub fn apply_args(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        env: &Rc<Environment>,
        action: Action
    ) -> Result<ArgsWrapper, ErrorPos>
    {
        if self.is_null()
        {
            Ok(ArgsWrapper::new())
        } else
        {
            let car = self.car();
            let cdr = self.cdr();

            let args = cdr.apply_args(state, memory, env, action)?;

            car.apply(state, memory, env, action)?;

            Ok(args.increment())
        }
    }

    pub fn eval(state: &mut State, env: &Rc<Environment>, ast: AstPos) -> Result<Self, ErrorPos>
    {
        if ast.is_list()
        {
            let op = Self::eval(state, env, ast.car())?;

            let args = ast.cdr();
            Self::eval_op(state, env, op, args)
        } else
        {
            Self::eval_atom(ast)
        }
    }

    pub fn eval_op(
        state: &mut State,
        env: &Rc<Environment>,
        op: Self,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let normal_eval = |op: Self, state, env, ast|
        {
            Ok(Self{
                position: op.position,
                expression: Expression::Application{
                    op: Box::new(op),
                    args: Box::new(Self::eval_args(state, env, ast)?)
                }
            })
        };

        if let Expression::Value(ref name) = op.expression
        {
            if let Some(on_eval) = state.primitives.get_by_name(name).as_ref().and_then(|primitive|
            {
                primitive.on_eval.as_ref().map(|x| x.clone())
            })
            {
                on_eval(op, state, env, ast)
            } else
            {
                normal_eval(op, state, env, ast)
            }
        } else
        {
            normal_eval(op, state, env, ast)
        }
    }

    pub fn eval_lambda(
        state: &mut State,
        env: &Rc<Environment>,
        args: AstPos
    ) -> Result<Self, ErrorPos>
    {
        Expression::argument_count_ast("lambda".to_owned(), 2, &args)?;

        let params = Box::new(Expression::ast_to_expression(args.car())?);
        let body = Box::new(Self::eval(state, env, args.cdr().car())?);

        Ok(Self{position: args.position, expression: Expression::Lambda{body, params}})
    }

    pub fn eval_args(
        state: &mut State,
        env: &Rc<Environment>,
        args: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if args.is_null()
        {
            return Ok(Self{position: args.position, expression: Expression::EmptyList});
        }

        let out = Expression::List{
            car: Box::new(Self::eval(state, env, args.car())?),
            cdr: Box::new(Self::eval_args(state, env, args.cdr())?)
        };

        Ok(Self{position: args.position, expression: out})
    }

    pub fn eval_atom(ast: AstPos) -> Result<Self, ErrorPos>
    {
        Ok(Self{
            position: ast.position,
            expression: Expression::eval_primitive_ast(ast.as_value()?)
        })
    }

    pub fn eval_sequence(
        state: &mut State,
        env: &Rc<Environment>,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        if ast.is_null()
        {
            return Err(ErrorPos{position: ast.position, error: Error::EmptySequence});
        }

        let car = Self::eval(state, env, ast.car())?;
        let cdr = ast.cdr();

        Ok(if cdr.is_null()
        {
            car
        } else
        {
            Self{
                position: ast.position,
                expression: Expression::Sequence{
                    first: Box::new(car),
                    after: Box::new(Self::eval_sequence(state, env, cdr)?)
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

    pub fn ast_to_expression(ast: AstPos) -> Result<ExpressionPos, ErrorPos>
    {
        let out = match ast.ast
        {
            Ast::Value(s) =>
            {
                let primitive = Ast::parse_primitive(&s)
                    .map_err(|error| ErrorPos{position: ast.position, error})?;

                Self::eval_primitive_ast(primitive)
            },
            Ast::EmptyList => Self::EmptyList,
            Ast::List{car, cdr} => Self::List{
                car: Box::new(Self::ast_to_expression(*car)?),
                cdr: Box::new(Self::ast_to_expression(*cdr)?)
            }
        };

        Ok(ExpressionPos{position: ast.position, expression: out})
    }

    pub fn eval_primitive_ast(primitive: PrimitiveType) -> Self
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
                error: Error::WrongArgumentsCount{proc: name, this_invoked: true, expected, got}
            })
        }
    }

    fn arg_count(args: &Self) -> usize
    {
        if args.is_null() || !args.is_list()
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
