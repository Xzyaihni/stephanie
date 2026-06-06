use std::{
    vec,
    iter,
    rc::Rc,
    io::{self, Write},
    fmt::{self, Debug, Display},
    collections::{VecDeque, HashMap},
    ops::{RangeInclusive, Add, Sub, Mul, Div, Rem, Index, IndexMut}
};

use strum::{EnumCount, FromRepr};

use crate::{
    debug_config::*,
    common::{DebugRaw, SeededRandom}
};

use super::{LispConfig, OutputWrapperRef};

pub use super::{
    Symbols,
    Error,
    ErrorPos,
    SymbolId,
    LispValue,
    LispMemory,
    ValueTag,
    OutputWrapper
};

pub use parser::{PrimitiveType, CodePosition, WithPosition, WithPositionMaybe, WithPositionTrait};

use parser::{Parser, Ast, AstPos};

mod parser;


pub const BEGIN_PRIMITIVE: &str = "begin";
pub const QUOTE_PRIMITIVE: &str = "quote";
pub const MAKE_VECTOR_PRIMITIVE: &str = "make-vector";
pub const VECTOR_SET_PRIMITIVE: &str = "vector-set!";
pub const CONS_PRIMITIVE: &str = "cons";
pub const CAR_PRIMITIVE: &str = "car";
pub const CDR_PRIMITIVE: &str = "cdr";

pub type OnApply = Rc<dyn Fn(&mut LispMemory, CodePosition, Register) -> Result<(), Error>>;
pub type OnEval = Rc<dyn Fn(&mut InterpretState, &mut LispMemory, AstPos) -> Result<InterRepr, ErrorPos>>;
pub type OnEvalState = Rc<dyn Fn(&mut InterpretState, &mut LispMemory) -> Result<(), ErrorPos>>;

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

impl ArgsCount
{
    pub fn contains(&self, value: usize) -> bool
    {
        match self
        {
            Self::Min(min) => value >= *min,
            Self::Between{start, end_inclusive} => (*start..=*end_inclusive).contains(&value),
            Self::Some(exact) => value == *exact
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PureCondition
{
    ArgsBetween{start: usize, end_inclusive: usize}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect
{
    Pure,
    PureIf(PureCondition),
    PureAllocating,
    Impure
}

impl<T> WithPositionTrait<Result<T, ErrorPos>> for Result<T, Error>
{
    fn with_position(self, position: CodePosition) -> Result<T, ErrorPos>
    {
        self.map_err(|value| ErrorPos{position, value})
    }
}

pub struct PrimitiveArgs<'a>
{
    pub position: CodePosition,
    pub memory: &'a mut LispMemory
}

impl<'a> PrimitiveArgs<'a>
{
    pub fn call_position(&self) -> CodePosition
    {
        self.position
    }
}

impl<'a> Iterator for PrimitiveArgs<'a>
{
    type Item = LispValue;

    fn next(&mut self) -> Option<Self::Item>
    {
        self.memory.try_pop_arg()
    }
}

fn simple_apply(f: impl Fn(PrimitiveArgs) -> Result<LispValue, Error> + 'static) -> OnApply
{
    Rc::new(move |memory, position, target|
    {
        let value = f(PrimitiveArgs{position, memory})?;
        memory.set_register(target, value);

        Ok(())
    })
}

#[derive(Clone)]
pub enum EvalKind
{
    Full(OnEval),
    State(OnEvalState)
}

#[derive(Clone)]
pub struct PrimitiveProcedureInfo
{
    args_count: ArgsCount,
    on_eval: Option<EvalKind>,
    on_apply: Option<(Effect, OnApply)>
}

impl PrimitiveProcedureInfo
{
    pub fn new(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_eval: OnEval,
        on_apply: impl Fn(PrimitiveArgs) -> Result<LispValue, Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(EvalKind::Full(on_eval)),
            on_apply: Some((effect, simple_apply(on_apply)))
        }
    }

    pub fn new_state(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_eval_state: OnEvalState,
        on_apply: impl Fn(PrimitiveArgs) -> Result<LispValue, Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(EvalKind::State(on_eval_state)),
            on_apply: Some((effect, simple_apply(on_apply)))
        }
    }

    pub fn new_eval(
        args_count: impl Into<ArgsCount>,
        on_eval: OnEval
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(EvalKind::Full(on_eval)),
            on_apply: None
        }
    }

    pub fn new_simple(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_apply: impl Fn(PrimitiveArgs) -> Result<LispValue, Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some((effect, simple_apply(on_apply)))
        }
    }

    pub fn new_with_target(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_apply: impl Fn(PrimitiveArgs, Register) -> Result<(), Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some((effect, Rc::new(move |memory, position, target|
            {
                on_apply(PrimitiveArgs{position, memory}, target)
            })))
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
pub struct Primitives
{
    indices: HashMap<String, u32>,
    primitives: Vec<PrimitiveProcedureInfo>
}

impl Default for Primitives
{
    fn default() -> Self
    {
        macro_rules! do_cond
        {
            ($name:literal, $f:expr) =>
            {
                ($name, PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, |mut args|
                {
                    let start = args.next().expect("must have 2 or more args");
                    args.try_fold((true, start), |(state, acc), x|
                    {
                        Self::call_op(acc, x, |a, b|
                        {
                            Some($f(a, b))
                        }, |a, b|
                        {
                            Some($f(a, b))
                        }).map(|next_state| (state && next_state, x))
                    }).map(|(state, _)| LispValue::new_bool(state))
                }))
            }
        }

        macro_rules! do_op
        {
            ($float_op:ident, $int_op:ident) =>
            {
                |mut args|
                {
                    let start = args.next().expect("must have 2 or more args");
                    args.try_fold(start, |acc, x|
                    {
                        Self::call_op(acc, x, |a, b|
                        {
                            Some(LispValue::new_integer(a.$int_op(b)?))
                        }, |a, b|
                        {
                            Some(LispValue::new_float(a.$float_op(b)))
                        })
                    })
                }
            }
        }

        macro_rules! do_op_simple
        {
            ($op:ident) =>
            {
                |mut args|
                {
                    Self::call_op(args.next().unwrap(), args.next().unwrap(), |a, b|
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
            ($name:literal, $($tag:expr),+) =>
            {
                ($name, PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let tag = args.next().unwrap().tag;
                    let is_equal = false $(|| tag == $tag)+;

                    Ok(is_equal.into())
                }))
            }
        }

        let (indices, primitives): (HashMap<String, _>, Vec<_>) = [
            (BEGIN_PRIMITIVE,
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(1), Rc::new(|interpret_state, memory, args|
                {
                    Ok(InterRepr::Sequence(InterReprPos::parse_args(interpret_state, memory, args)?))
                }))),
            (QUOTE_PRIMITIVE,
                PrimitiveProcedureInfo::new_eval(1, Rc::new(|_interpret_state, _memory, args|
                {
                    Ok(InterRepr::Quoted(args.car()))
                }))),
            ("error", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();

                    Err(Error::Custom(arg.as_string(args.memory)?))
                })),
            ("if",
                PrimitiveProcedureInfo::new_eval(2..=3, Rc::new(|interpret_state, memory, args|
                {
                    InterRepr::parse_if(interpret_state, memory, args)
                }))),
            ("cond",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(2), Rc::new(|interpret_state, memory, args|
                {
                    fn parse_clause(interpret_state: &mut InterpretState, memory: &mut LispMemory, clause: AstPos) -> Result<(InterReprPos, InterReprPos), ErrorPos>
                    {
                        if !clause.is_list() || clause.is_null()
                        {
                            return Err(ErrorPos{position: clause.position, value: Error::ExpectedList});
                        }

                        let check = InterReprPos::parse(interpret_state, memory, clause.car())?;

                        let tail = clause.cdr();
                        let tail_position = tail.position;
                        let then = InterRepr::Sequence(InterReprPos::parse_args(interpret_state, memory, tail)?);

                        Ok((check, then.with_position(tail_position)))
                    }

                    fn parse_rest(interpret_state: &mut InterpretState, memory: &mut LispMemory, clauses: AstPos) -> Result<InterReprPos, ErrorPos>
                    {
                        if !clauses.is_list() || clauses.is_null()
                        {
                            return Err(ErrorPos{position: clauses.position, value: Error::ExpectedList});
                        }

                        let this = clauses.car();
                        let this_position = this.position;

                        let (check, then) = parse_clause(interpret_state, memory, this)?;

                        let rest = clauses.cdr();

                        debug_assert!(rest.is_list(), "malformed ast");

                        let else_body = if rest.is_null()
                        {
                            InterRepr::Value(LispValue::new_empty_list()).with_position(rest.position)
                        } else
                        {
                            parse_rest(interpret_state, memory, rest)?
                        };

                        Ok(InterRepr::If{
                            check: Box::new(check),
                            then: Box::new(then),
                            else_body: Box::new(else_body)
                        }.with_position(this_position))
                    }

                    parse_rest(interpret_state, memory, args).map(|x| x.value)
                }))),
            (CONS_PRIMITIVE,
                PrimitiveProcedureInfo::new_simple(2, Effect::PureAllocating, |mut args|
                {
                    let restore = args.memory.with_saved_registers([Register::Value]);

                    let car = args.next().unwrap();
                    args.memory.set_register(Register::Temporary, car);

                    let cdr = args.next().unwrap();
                    args.memory.set_register(Register::Value, cdr);

                    args.memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    let value = args.memory.get_register(Register::Value);
                    restore(args.memory)?;

                    Ok(value)
                })),
            (CAR_PRIMITIVE,
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = args.memory.get_car(arg.as_list_id()?);

                    Ok(value)
                })),
            (CDR_PRIMITIVE,
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = args.memory.get_cdr(arg.as_list_id()?);

                    Ok(value)
                })),
            ("set-car!",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let list_id = arg.as_list_id()?;

                    let value = args.next().unwrap();

                    args.memory.set_car(list_id, value);

                    Ok(().into())
                })),
            ("set-cdr!",
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let list_id = arg.as_list_id()?;

                    let value = args.next().unwrap();

                    args.memory.set_cdr(list_id, value);

                    Ok(().into())
                })),
            ("+", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(add, checked_add))),
            ("-", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(sub, checked_sub))),
            ("*", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(mul, checked_mul))),
            ("/", PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(div, checked_div))),
            ("remainder", PrimitiveProcedureInfo::new_simple(2, Effect::Pure, do_op_simple!(rem))),
            do_cond!("=", |a, b| a == b),
            do_cond!(">", |a, b| a > b),
            do_cond!("<", |a, b| a < b),
            ("eq?", PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
            {
                let a = args.next().unwrap();
                let b = args.next().unwrap();

                Ok((a.value == b.value).into())
            })),
            is_tag!("symbol?", ValueTag::Symbol),
            is_tag!("pair?", ValueTag::List),
            is_tag!("null?", ValueTag::EmptyList),
            is_tag!("char?", ValueTag::Char),
            is_tag!("boolean?", ValueTag::Bool),
            is_tag!("vector?", ValueTag::Vector),
            is_tag!("number?", ValueTag::Integer, ValueTag::Float),
            ("procedure?", PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
            {
                let value = args.next().unwrap();

                let is_compound = ||
                {
                    value.as_list_id().map(|x|
                    {
                        args.memory.get_cdr(x).tag == ValueTag::Address
                    }).unwrap_or(false)
                };

                let is_equal = value.tag == ValueTag::PrimitiveProcedure || is_compound();

                Ok(is_equal.into())
            })),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(2), Rc::new(|interpret_state, memory, args|
                {
                    InterRepr::parse_lambda(interpret_state, memory, "<lambda>".to_owned(), args)
                }))),
            ("define",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(2), Rc::new(|interpret_state, memory, args: AstPos|
                {
                    let first = args.car();

                    let is_procedure = first.is_list();

                    let position = args.position;

                    let (name, value) = if is_procedure
                    {
                        if first.is_null()
                        {
                            return Err(ErrorPos{position, value: Error::DefineEmptyList});
                        }

                        let name = InterReprPos::parse_symbol(memory, &first.car())?;

                        let lambdas_body = AstPos::cons(first.cdr(), args.cdr());

                        let lambda_name = memory.get_symbol(name);
                        let lambda = InterRepr::parse_lambda(interpret_state, memory, lambda_name, lambdas_body)?
                            .with_position(position);

                        (name, lambda)
                    } else
                    {
                        let name = InterReprPos::parse_symbol(memory, &first)?;
                        let args = InterReprPos::parse_args(interpret_state, memory, args.cdr())?;

                        let value = args.into_iter().next().unwrap();

                        (name, value)
                    };

                    {
                        let name = memory.get_symbol(name);
                        if memory.primitives.get_by_name(&name).is_some()
                        {
                            return Err(ErrorPos{
                                position,
                                value: Error::AttemptedShadowing(name)
                            });
                        }
                    }

                    Ok(InterRepr::Define(DefineStage::Parsed{
                        name,
                        body: Box::new(value)
                    }))
                }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|interpret_state, memory, args|
                {
                    InterRepr::parse_let(interpret_state, memory, args)
                }))),
            ("eval",
                PrimitiveProcedureInfo::new_state(1, Effect::Impure, Rc::new(|interpret_state: &mut InterpretState, _memory|
                {
                    interpret_state.eval_encountered = true;

                    Ok(())
                }), |mut args|
                {
                    let value = args.next().unwrap().as_symbol_id()?;

                    let memory = args.memory;

                    if let Some(x) = memory.lookup_symbol(memory.get_register(Register::Environment), value)
                    {
                        Ok(x)
                    } else
                    {
                        let name = memory.get_symbol(value);
                        memory.primitives.index_by_name(&name).map(|index|
                        {
                            LispValue::new_primitive_procedure(index)
                        }).ok_or(Error::UndefinedVariable(name))
                    }
                })),
            (MAKE_VECTOR_PRIMITIVE,
                PrimitiveProcedureInfo::new_with_target(2, Effect::PureAllocating, |mut args, target|
                {
                    let len = args.next().unwrap().as_integer()? as usize;
                    let fill = args.next().unwrap();

                    args.memory.make_vector(target, vec![fill; len])
                })),
            (VECTOR_SET_PRIMITIVE,
                PrimitiveProcedureInfo::new_simple(3, Effect::Impure, |mut args|
                {
                    let vec = args.next().unwrap();
                    let index = args.next().unwrap();
                    let value = args.next().unwrap();

                    let vec = vec.as_vector_mut(args.memory)?;

                    let index = index.as_integer()?;

                    *vec.get_mut(index as usize)
                        .ok_or(Error::IndexOutOfRange(index))? = value;

                    Ok(().into())
                })),
            ("vector-ref",
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let vec = args.next().unwrap();
                    let index = args.next().unwrap();

                    let vec = vec.as_vector_ref(args.memory)?;
                    let index = index.as_integer()?;

                    let value = *vec.get(index as usize).ok_or(Error::IndexOutOfRange(index))?;

                    Ok(value)
                })),
            ("vector-length",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let vec = args.next().unwrap();

                    let vec = vec.as_vector_ref(args.memory)?;

                    Ok((vec.len() as i32).into())
                })),
            ("display",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();

                    print!("{}", arg.to_string(args.memory));
                    io::stdout().flush().unwrap();

                    Ok(().into())
                })),
            ("newline",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, |_args|
                {
                    println!();

                    Ok(().into())
                })),
            ("random-integer-seeded",
                PrimitiveProcedureInfo::new_simple(ArgsCount::Between{start: 1, end_inclusive: 2}, Effect::Pure, |mut args|
                {
                    let seed = args.next().unwrap().as_integer()?;
                    let mut random_generator = SeededRandom::from(seed as u64);

                    let value = if let Some(limit) = args.next()
                    {
                        let limit = limit.as_integer()?;

                        if limit <= 0
                        {
                            return Ok(0.into());
                        }

                        random_generator.next_u64_between(0..limit as u64) as i32
                    } else
                    {
                        random_generator.next_u64_between(0..i32::MAX as u64) as i32
                    };

                    Ok(value.into())
                })),
            ("random-integer",
                PrimitiveProcedureInfo::new_simple(ArgsCount::Between{start: 0, end_inclusive: 1}, Effect::Impure, |mut args|
                {
                    let value = if let Some(limit) = args.next()
                    {
                        let limit = limit.as_integer()?;

                        if limit <= 0
                        {
                            return Ok(0.into());
                        }

                        fastrand::i32(0..limit)
                    } else
                    {
                        fastrand::i32(0..)
                    };

                    Ok(value.into())
                })),
            ("random-float",
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, |_args|
                {
                    Ok(fastrand::f32().into())
                })),
            ("floor",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    Ok(args.next().unwrap().as_float()?.floor().into())
                })),
            ("wrapping-add",
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let a = args.next().unwrap().as_integer()?;
                    let b = args.next().unwrap().as_integer()?;

                    Ok(a.wrapping_add(b).into())
                })),
            ("exact->inexact",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();

                    let value = if arg.tag == ValueTag::Float
                    {
                        arg
                    } else
                    {
                        let number = arg.as_integer()?;

                        (number as f32).into()
                    };

                    Ok(value)
                })),
            ("inexact->exact",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();

                    let value = if arg.tag == ValueTag::Integer
                    {
                        arg
                    } else
                    {
                        let number = arg.as_float()?;

                        (number.round() as i32).into()
                    };

                    Ok(value)
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
}

impl Primitives
{
    pub fn add(&mut self, name: impl Into<String>, procedure: PrimitiveProcedureInfo)
    {
        let name = name.into();

        let id = self.primitives.len();

        self.primitives.push(procedure);
        self.indices.insert(name, id as u32);
    }

    pub fn replace(&mut self, name: &str, procedure: PrimitiveProcedureInfo)
    {
        let id = self.indices.get(name).unwrap_or_else(||
        {
            panic!("tried to replace primitive procedure `{name}`, but it didnt exist");
        });

        self.primitives[*id as usize] = procedure;
    }

    pub fn iter_infos(&self) -> impl Iterator<Item=(&String, ArgsCount)>
    {
        self.indices.iter().map(|(name, index)|
        {
            (name, self.primitives[*index as usize].args_count)
        })
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

    fn call_op<T, FI, FF>(
        a: LispValue,
        b: LispValue,
        op_integer: FI,
        op_float: FF
    ) -> Result<T, Error>
    where
        FI: Fn(i32, i32) -> Option<T>,
        FF: Fn(f32, f32) -> Option<T>
    {
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
}

#[derive(Debug, Clone)]
enum Command
{
    Push(Register),
    Pop(Register),
    PutValue{value: LispValue, register: Register},
    Move{target: Register, source: Register},
    Lookup{location: LexicalAddress, register: Register},
    LookupOuter{id: SymbolId, register: Register},
    Define{id: SymbolId, register: Register},
    CreateChildEnvironment,
    PutLabel{target: Register, label: Label},
    Jump(Label),
    JumpRegister(Register),
    JumpIfTrue{target: Label, check: Register},
    JumpIfFalse{target: Label, check: Register},
    IsTag{check: Register, tag: ValueTag},
    Cons{target: Register, car: Register, cdr: Register},
    Car{target: Register, source: Register},
    Cdr{target: Register, source: Register},
    CallPrimitiveValue{target: Register},
    CallPrimitiveValueUnchecked{target: Register},
    Error(ErrorPos),
    Label(Label)
}

struct CommandDisplay<'a>
{
    memory: &'a LispMemory,
    value: &'a Command
}

impl Debug for CommandDisplay<'_>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self.value
        {
            Command::PutValue{value, register} =>
            {
                f.debug_struct("PutValue")
                    .field("value", &DebugRaw(value.to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            Command::LookupOuter{id, register} =>
            {
                f.debug_struct("LookupOuter")
                    .field("id", &DebugRaw(LispValue::new_symbol_raw(*id).to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            Command::Define{id, register} =>
            {
                f.debug_struct("Define")
                    .field("id", &DebugRaw(LispValue::new_symbol_raw(*id).to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            x => write!(f, "{x:?}")
        }
    }
}

impl Command
{
    pub fn modifies_registers(&self) -> Vec<Register>
    {
        match self
        {
            Self::PutValue{register, ..}
            | Self::Lookup{register, ..}
            | Self::LookupOuter{register, ..}
            | Self::PutLabel{target: register, ..}
            | Self::Move{target: register, ..}
            | Self::Pop(register)
            | Self::Cons{target: register, ..}
            | Self::Car{target: register, ..}
            | Self::Cdr{target: register, ..}
            | Self::CallPrimitiveValue{target: register, ..}
            | Self::CallPrimitiveValueUnchecked{target: register, ..} => vec![*register],
            Self::CreateChildEnvironment => vec![Register::Environment, Register::Value, Register::Temporary],
            Self::Define{..} => vec![Register::Value, Register::Temporary],
            Self::IsTag{..} => vec![Register::Temporary],
            Self::Push(_)
            | Self::Jump(_)
            | Self::JumpRegister(_)
            | Self::JumpIfTrue{..}
            | Self::JumpIfFalse{..}
            | Self::Error(_)
            | Self::Label(_) => Vec::new()
        }
    }

    pub fn is_label(&self) -> bool
    {
        if let Self::Label(_) = self
        {
            true
        } else
        {
            false
        }
    }

    pub fn into_raw(self, labels: &HashMap<Label, usize>) -> CommandRaw
    {
        match self
        {
            Self::Push(register) => CommandRaw::Push(register),
            Self::Pop(register) => CommandRaw::Pop(register),
            Self::PutValue{value, register} => CommandRaw::PutValue{value, register},
            Self::Move{target, source} => CommandRaw::Move{target, source},
            Self::Lookup{location, register} => CommandRaw::Lookup{location, register},
            Self::LookupOuter{id, register} => CommandRaw::LookupOuter{id, register},
            Self::Define{id, register} => CommandRaw::Define{id, register},
            Self::CreateChildEnvironment => CommandRaw::CreateChildEnvironment,
            Self::PutLabel{target, label} =>
            {
                CommandRaw::PutValue{
                    value: LispValue::new_address(*labels.get(&label).unwrap() as u32),
                    register: target
                }
            },
            Self::Jump(label) => CommandRaw::Jump(*labels.get(&label).unwrap()),
            Self::JumpRegister(register) => CommandRaw::JumpRegister(register),
            Self::JumpIfTrue{target, check} => CommandRaw::JumpIfTrue{target: *labels.get(&target).unwrap(), check},
            Self::JumpIfFalse{target, check} => CommandRaw::JumpIfFalse{target: *labels.get(&target).unwrap(), check},
            Self::IsTag{check, tag} => CommandRaw::IsTag{check, tag},
            Self::Cons{target, car, cdr} => CommandRaw::Cons{target, car, cdr},
            Self::Car{target, source} => CommandRaw::Car{target, source},
            Self::Cdr{target, source} => CommandRaw::Cdr{target, source},
            Self::CallPrimitiveValue{target} => CommandRaw::CallPrimitiveValue{target},
            Self::CallPrimitiveValueUnchecked{target} => CommandRaw::CallPrimitiveValueUnchecked{target},
            Self::Error(err) => CommandRaw::Error(err),
            Self::Label(_) => unreachable!("labels have no raw equivalent")
        }
    }
}

type CommandPos = WithPositionMaybe<Command>;

#[derive(Debug)]
struct CompiledPart
{
    modifies: RegisterStates,
    requires: RegisterStates,
    commands: Vec<CommandPos>
}

impl From<Command> for CompiledPart
{
    fn from(command: Command) -> Self
    {
        Self::from(CommandPos::from(command))
    }
}

impl From<CommandPos> for CompiledPart
{
    fn from(command: CommandPos) -> Self
    {
        Self::from_commands(vec![command])
    }
}

impl CompiledPart
{
    pub fn new() -> Self
    {
        Self::from_commands(Vec::new())
    }

    pub fn from_commands(commands: Vec<CommandPos>) -> Self
    {
        let mut modifies = RegisterStates::none();

        commands.iter().for_each(|CommandPos{value, ..}|
        {
            value.modifies_registers().into_iter().for_each(|register|
            {
                modifies[register] = true;
            });
        });

        Self{
            modifies,
            requires: RegisterStates::none(),
            commands
        }
    }

    pub fn with_requires(mut self, requires: RegisterStates) -> Self
    {
        self.requires = self.requires.union(requires);

        self
    }

    pub fn with_modifies(mut self, modifies: RegisterStates) -> Self
    {
        self.modifies = self.modifies.union(modifies);

        self
    }

    pub fn combine(self, other: impl Into<Self>) -> Self
    {
        self.combine_preserving(other, RegisterStates::none())
    }

    pub fn combine_preserving(self, other: impl Into<Self>, registers: RegisterStates) -> Self
    {
        let other = other.into();

        let save = other.requires.intersection(self.modifies).intersection(registers);

        let save_registers = save.into_iter().filter(|(_, x)| *x);

        let commands = save_registers.clone().map(|(register, _)| -> CommandPos
            {
                Command::Push(register).into()
            })
            .chain(self.commands)
            .chain(save_registers.rev().map(|(register, _)| -> CommandPos
            {
                Command::Pop(register).into()
            }))
            .chain(other.commands)
            .collect();

        Self{
            modifies: self.modifies.difference(save).union(other.modifies),
            requires: self.requires.union(other.requires.difference(self.modifies.difference(save))),
            commands
        }
    }

    pub fn with_proceed(self, proceed: Proceed) -> Self
    {
        self.combine_preserving(proceed.into_compiled(), RegisterStates::one(Register::Return))
    }

    fn print_program(&self, memory: &LispMemory)
    {
        let mut offset = 0;
        self.commands.iter().enumerate().for_each(|(index, WithPositionMaybe{value, position})|
        {
            let is_label = value.is_label();

            if is_label
            {
                offset += 1;
            }

            if !is_label
            {
                eprint!("{}: ", index - offset);
            }

            eprint!("{:?}", CommandDisplay{memory, value});

            if let Some(position) = position
            {
                eprint!(" (source#{} {position})", position.source);
            }

            eprintln!();
        });
    }

    fn verify_program(commands: &[CommandPos], memory: &LispMemory) -> Result<(), ErrorPos>
    {
        commands.iter().try_for_each(|CommandPos{position, value}|
        {
            if let Command::Define{id, ..} = value
            {
                let name = memory.get_symbol(*id);

                if memory.primitives.get_by_name(&name).is_some()
                {
                    return Err(ErrorPos{
                        position: position.expect("define must have a position"),
                        value: Error::AttemptedShadowing(name)
                    });
                }
            }

            Ok(())
        })
    }

    pub fn into_program(mut self, state: CompileState) -> Result<CompiledProgram, ErrorPos>
    {
        state.lambdas.into_iter().for_each(|lambda|
        {
            self.commands.extend(lambda.commands);
        });

        self.commands.push(Command::Label(Label::Halt).into());

        if DebugConfig::is_enabled(DebugTool::Lisp)
        {
            self.print_program(state.memory);
        }

        Self::verify_program(&self.commands, state.memory)?;

        let labels = {
            let mut filtered_labels = 0;

            self.commands.iter().enumerate().filter_map(|(index, WithPositionMaybe{value: command, ..})|
            {
                if let Command::Label(label) = command
                {
                    let address = index - filtered_labels;
                    filtered_labels += 1;

                    Some((*label, address))
                } else
                {
                    None
                }
            }).collect::<HashMap<Label, usize>>()
        };

        let (positions, mut commands): (Vec<_>, Vec<_>) = self.commands.into_iter().filter(|command|
        {
            !command.is_label()
        }).map(|WithPositionMaybe{position, value: command}|
        {
            (position, command.into_raw(&labels))
        }).unzip();

        while let Some(CommandRaw::Jump(location)) = commands.last()
        {
            if *location == commands.len()
            {
                commands.pop();
            } else
            {
                break;
            }
        }

        Ok(CompiledProgram{
            positions,
            commands
        })
    }
}

pub type InterReprPos = WithPosition<InterRepr>;

#[derive(Debug, Clone)]
pub enum LambdaParams
{
    Variadic(WithPosition<SymbolId>),
    Normal(Vec<WithPosition<SymbolId>>)
}

impl LambdaParams
{
    pub fn parse(
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        match &ast.value
        {
            Ast::Allocated(_) => Err(ErrorPos{position: ast.position, value: Error::ExpectedParams}),
            Ast::Value(_) =>
            {
                Ok(Self::Variadic(InterReprPos::parse_symbol(memory, &ast)?.with_position(ast.position)))
            },
            Ast::List{..} => Ok(Self::Normal(Self::parse_list(memory, ast)?)),
            Ast::EmptyList => Ok(Self::Normal(Vec::new()))
        }
    }

    pub fn parse_list(memory: &mut LispMemory, ast: AstPos) -> Result<Vec<WithPosition<SymbolId>>, ErrorPos>
    {
        match ast.value
        {
            Ast::List{car, cdr} =>
            {
                let position = car.position;

                let tail = Self::parse_list(memory, *cdr)?;
                let symbol = InterReprPos::parse_symbol(memory, &car)?;

                Ok(iter::once(WithPosition{position, value: symbol}).chain(tail).collect())
            },
            Ast::EmptyList => Ok(Vec::new()),
            Ast::Value(_)
            | Ast::Allocated(_) => unreachable!("malformed ast")
        }
    }

    fn add_to_compile_env(&self, interpret_state: &mut InterpretState)
    {
        match self
        {
            Self::Variadic(param) =>
            {
                interpret_state.compile_env_add(param.value, false, None);
            },
            Self::Normal(params) =>
            {
                params.iter().for_each(|param|
                {
                    interpret_state.compile_env_add(param.value, false, None);
                });
            }
        }
    }

    fn compile(
        self,
        state: &mut CompileState,
        name: String
    ) -> CompiledPart
    {
        match self
        {
            Self::Variadic(WithPosition{position, value: id}) => CompiledPart::from_commands(vec![CommandPos{
                position: Some(position),
                value: Command::Define{id, register: Register::Argument}
            }]),
            Self::Normal(params) =>
            {
                let amount = params.len();
                let commands = params.into_iter().enumerate().flat_map(|(index, WithPosition{position, value: param})|
                {
                    let is_last = amount == (index + 1);

                    let define_one = |include_tail|
                    {
                        let mut commands = vec![
                            Command::Car{target: Register::Temporary, source: Register::Argument}.into(),
                            CommandPos{
                                position: Some(position),
                                value: Command::Define{id: param, register: Register::Temporary}
                            }
                        ];

                        if include_tail
                        {
                            commands.push(Command::Cdr{target: Register::Argument, source: Register::Argument}.into());
                        }

                        commands
                    };

                    let include_tail = state.type_checks || !is_last;

                    let commands = define_one(include_tail);

                    if state.type_checks
                    {
                        let after_little_error = Label::AfterError(state.label_id());

                        let mut commands: Vec<_> = [
                            Command::IsTag{check: Register::Argument, tag: ValueTag::List}.into(),
                            Command::JumpIfTrue{target: after_little_error, check: Register::Temporary}.into(),
                            Command::Error(ErrorPos{position, value: Error::WrongArgumentsCount{
                                proc: name.clone(),
                                expected: amount.to_string(),
                                got: index
                            }}).into(),
                            Command::Label(after_little_error).into()
                        ].into_iter().chain(commands).collect();

                        if is_last
                        {
                            let after_error = Label::AfterError(state.label_id());

                            commands.extend([
                                Command::IsTag{check: Register::Argument, tag: ValueTag::EmptyList}.into(),
                                Command::JumpIfTrue{target: after_error, check: Register::Temporary}.into(),
                                Command::Error(ErrorPos{position, value: Error::WrongArgumentsCount{
                                    proc: name.clone(),
                                    expected: amount.to_string(),
                                    got: amount
                                }}).into(),
                                Command::Label(after_error).into()
                            ]);
                        }

                        commands
                    } else
                    {
                        commands
                    }
                }).collect::<Vec<_>>();

                CompiledPart::from_commands(commands)
            }
        }
    }
}

type MarkStackFunc<'a> = Box<dyn FnOnce(
    &mut Vec<MarkStackEntry<'a>>,
    &mut InterpretState,
    &mut Vec<EnvPosition>,
    &mut HashMap<EnvPosition, LambdaDefineInfo<'a>>,
    &mut VecDeque<(u32, &'a InterReprPos)>,
    bool
) -> bool + 'a>;

struct MarkStackEntry<'a>
{
    func: MarkStackFunc<'a>
}

#[derive(Debug, Clone)]
pub enum DefineStage
{
    Parsed{name: SymbolId, body: Box<InterReprPos>},
    Processed{name: SymbolId, position: EnvPosition, body: Box<InterReprPos>}
}

#[derive(Debug, Clone)]
pub enum LookupStage
{
    Parsed{symbol: SymbolId},
    Processed{symbol: SymbolId, pos: CompileEnvPosition},
    Final{symbol: SymbolId, pos: CompileEnvPosition, location: Option<LexicalAddress>}
}

#[derive(Debug, Clone)]
pub enum InterRepr
{
    Apply{op: Box<InterReprPos>, args: Vec<InterReprPos>},
    Sequence(Vec<InterReprPos>),
    If{check: Box<InterReprPos>, then: Box<InterReprPos>, else_body: Box<InterReprPos>},
    Define(DefineStage),
    Lambda{name: String, params: LambdaParams, body: Box<InterReprPos>},
    Quoted(AstPos),
    Lookup(LookupStage),
    Value(LispValue)
}

impl InterReprPos
{
    fn parse_symbol(memory: &mut LispMemory, ast: &AstPos) -> Result<SymbolId, ErrorPos>
    {
        let position = ast.position;
        Self::parse_primitive_value(memory, ast).and_then(|x| x.as_symbol_id().with_position(position))
    }

    fn parse_primitive_value(memory: &mut LispMemory, ast: &AstPos) -> Result<LispValue, ErrorPos>
    {
        if let Ast::Value(ref x) = ast.value
        {
            InterReprPos::parse_primitive_text(memory, x).with_position(ast.position)
        } else
        {
            Err(ErrorPos{position: ast.position, value: Error::ExpectedSymbol})
        }
    }

    fn parse_primitive_text(memory: &mut LispMemory, text: &str) -> Result<LispValue, Error>
    {
        Ok(memory.new_primitive_value(Ast::parse_primitive(text)?))
    }

    fn compile_env_lookup_with<T>(
        pos: &CompileEnvPosition,
        mut try_at: impl FnMut(EnvPosition) -> Option<T>
    ) -> Option<T>
    {
        for env_position in iter::once(pos.pos).chain(pos.indices.iter().copied().rev())
        {
            let result = try_at(env_position);

            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    fn compile_env_lookup_position(
        compile_env: &[Vec<Vec<(SymbolId, CompileSymbolState)>>],
        symbol: SymbolId,
        pos: &CompileEnvPosition
    ) -> Option<EnvPosition>
    {
        Self::compile_env_lookup_with(pos, |EnvPosition{depth, lambda_index, index}|
        {
            let this_env = &compile_env[depth][lambda_index][..index];

            this_env.iter().rposition(|(key, _)| symbol == *key).map(|found_index|
            {
                EnvPosition{depth, lambda_index, index: found_index + 1}
            })
        })
    }

    fn compile_env_lookup(
        interpret_state: &mut InterpretState,
        id: SymbolId,
        pos: &CompileEnvPosition
    ) -> (Option<EnvPosition>, Option<LispValue>)
    {
        enum FoundResult
        {
            Value(LispValue),
            Lookup(EnvPosition)
        }

        let found: Option<FoundResult> = Self::compile_env_lookup_with(pos, |EnvPosition{depth, lambda_index, index}|
        {
            let this_env = &mut interpret_state.compile_env[depth][lambda_index][..index];

            this_env.iter().rposition(|(key , _)| id == *key).map(|symbol_index|
            {
                let this_symbol = &mut this_env[symbol_index].1;

                if let Some(value) = this_symbol.value
                {
                    return FoundResult::Value(value);
                }

                this_symbol.looked_up = this_symbol.looked_up.saturating_add(1);

                FoundResult::Lookup(EnvPosition{depth, lambda_index, index: symbol_index + 1})
            })
        });

        let (position, is_found) = match found
        {
            None => (None, false),
            Some(FoundResult::Lookup(position)) => (Some(position), true),
            Some(FoundResult::Value(value)) => return (None, Some(value))
        };

        if !is_found
        {
            interpret_state.outer_lookups.push(id);
        }

        (position, None)
    }

    fn parse(
        interpret_state: &mut InterpretState,
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        match ast.value
        {
            Ast::Value(_) =>
            {
                let value = Self::parse_primitive_value(memory, &ast)?;

                Ok(if let Ok(id) = value.as_symbol_id()
                {
                    if let Some(primitive_id) = memory.primitives.index_by_name(&memory.get_symbol(id))
                    {
                        InterRepr::Value(LispValue::new_primitive_procedure(primitive_id))
                    } else if id == interpret_state.debug_mode_symbol
                    {
                        InterRepr::Value(LispValue::new_bool(interpret_state.debug_mode))
                    } else
                    {
                        InterRepr::Lookup(LookupStage::Parsed{symbol: id})
                    }
                } else
                {
                    InterRepr::Value(value)
                }.with_position(ast.position))
            },
            Ast::Allocated(_) =>
            {
                let position = ast.position;
                Ok(InterRepr::Quoted(ast).with_position(position))
            },
            Ast::EmptyList => Ok(InterRepr::Value(LispValue::new_empty_list()).with_position(ast.position)),
            Ast::List{car, cdr} =>
            {
                let op = Self::parse(interpret_state, memory, *car)?;

                if let InterRepr::Value(value) = op.value
                {
                    if let Ok(id) = value.as_primitive_procedure()
                    {
                        let primitive = memory.primitives.get(id).clone();
                        if let Some(on_eval) = &primitive.on_eval
                        {
                            match on_eval
                            {
                                EvalKind::Full(on_eval) =>
                                {
                                    let args = *cdr;

                                    let args_count = args.list_length();
                                    if !primitive.args_count.contains(args_count)
                                    {
                                        return Err(Error::WrongArgumentsCount{
                                            proc: memory.primitives.name_by_index(id).to_owned(),
                                            expected: primitive.args_count.to_string(),
                                            got: args_count
                                        }).with_position(ast.position);
                                    }

                                    return on_eval(interpret_state, memory, args).map(|x| x.with_position(ast.position));
                                },
                                EvalKind::State(on_eval_state) =>
                                {
                                    on_eval_state(interpret_state, memory)?;
                                }
                            }
                        }
                    }
                }

                let args = Self::parse_args(interpret_state, memory, *cdr)?;

                if let InterRepr::Lookup(LookupStage::Parsed{symbol: this_lookup}) = *op
                {
                    if let Some((load_symbol, source_id, interpret_state_fn)) = &interpret_state.load_handler
                    {
                        if this_lookup == *load_symbol
                        {
                            let filename_repr = if let Some(x) = args.into_iter().next()
                            {
                                x
                            } else
                            {
                                return Err(Error::WrongArgumentsCount{
                                    proc: "load".to_owned(),
                                    expected: 1.to_string(),
                                    got: 0
                                }).with_position(ast.position);
                            };

                            let filename_ast = if let InterRepr::Quoted(x) = filename_repr.value
                            {
                                x
                            } else
                            {
                                return Err(Error::Custom("expected filepath string".to_owned())).with_position(ast.position);
                            };

                            let filename = if let Ast::Allocated(x) = filename_ast.value
                            {
                                x
                            } else
                            {
                                return Err(Error::Custom("expected filepath string".to_owned())).with_position(ast.position);
                            };

                            let loaded_code = if let Some(x) = interpret_state_fn(&filename)
                            {
                                x
                            } else
                            {
                                return Err(Error::Custom(format!("error when loading {filename}"))).with_position(ast.position);
                            };

                            let loaded_ast = Parser::parse(*source_id, &[&loaded_code])?;

                            return InterReprPos::parse(
                                interpret_state,
                                memory,
                                loaded_ast
                            );
                        }
                    }
                }

                Ok(InterRepr::Apply{op: Box::new(op), args}.with_position(ast.position))
            }
        }
    }

    fn parse_args(
        interpret_state: &mut InterpretState,
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Vec<Self>, ErrorPos>
    {
        match ast.value
        {
            Ast::Value(_)
            | Ast::Allocated(_) => unreachable!("malformed ast"),
            Ast::EmptyList => Ok(Vec::new()),
            Ast::List{car, cdr} =>
            {
                let first = Self::parse(interpret_state, memory, *car)?;

                Ok(iter::once(first).chain(Self::parse_args(interpret_state, memory, *cdr)?).collect())
            }
        }
    }

    fn is_known_primitive(&self) -> bool
    {
        if let InterRepr::Value(value) = self.value
        {
            value.as_primitive_procedure().is_ok()
        } else
        {
            false
        }
    }

    fn is_known_compound(&self, state: &CompileState) -> bool
    {
        if let InterRepr::Lambda{..} = self.value
        {
            true
        } else if let InterRepr::Lookup(LookupStage::Final{symbol, pos, ..}) = &self.value
        {
            if let Some(pos) = Self::compile_env_lookup_position(state.compile_env, *symbol, pos)
            {
                state.compile_env[pos.depth][pos.lambda_index][pos.index - 1].1.is_lambda
            } else
            {
                false
            }
        } else
        {
            false
        }
    }

    fn remove_unused_defines(
        &mut self,
        interpret_state: &mut InterpretState
    ) -> bool
    {
        match &mut self.value
        {
            InterRepr::Apply{op, args} =>
            {
                op.remove_unused_defines(interpret_state);

                args.iter_mut().for_each(|arg|
                {
                    arg.remove_unused_defines(interpret_state);
                });
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.retain_mut(|value|
                {
                    let is_used = value.remove_unused_defines(interpret_state);

                    is_used
                });
            },
            InterRepr::If{check, then, else_body} =>
            {
                check.remove_unused_defines(interpret_state);
                then.remove_unused_defines(interpret_state);
                else_body.remove_unused_defines(interpret_state);
            },
            InterRepr::Define(DefineStage::Processed{name, position: p, body}) =>
            {
                body.remove_unused_defines(interpret_state);

                let result = interpret_state.compile_env[p.depth][p.lambda_index][..p.index].iter().rev().find_map(|(key, value)|
                {
                    (key == name).then(||
                    {
                        let replaced = value.value.is_some();
                        let is_unused = value.looked_up == 0 || replaced;

                        is_unused
                    })
                });

                let is_removed = result.is_none();
                let is_unused = if p.depth == 0 && interpret_state.outer_lookups.contains(name)
                {
                    false
                } else
                {
                    result.unwrap_or(true)
                };

                if is_unused
                {
                    if !is_removed
                    {
                        let env = &mut interpret_state.compile_env[p.depth][p.lambda_index];
                        let index = env[..p.index].iter().rposition(|(key, _value)|
                        {
                            key == name
                        }).expect("must be defined");

                        env[index].1.mark_removed = true;
                    }

                    return false;
                }
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{name: _, params: _, body} =>
            {
                body.remove_unused_defines(interpret_state);
            },
            InterRepr::Quoted(_) => (),
            InterRepr::Lookup(_) => (),
            InterRepr::Value(_) => ()
        }

        true
    }

    fn mark_lookups<'a>(
        &'a self,
        call_stack: &mut Vec<MarkStackEntry<'a>>,
        possibly_op: u32
    ) -> bool
    {
        match &self.value
        {
            InterRepr::Apply{op, args} =>
            {
                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, explore_queue, previous_skipped|
                    {
                        if previous_skipped
                        {
                            while let Some((this_possibly_op, this_next)) = explore_queue.pop_front()
                            {
                                this_next.mark_lookups(call_stack, this_possibly_op);
                            }
                        }

                        previous_skipped
                    })
                });

                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                    {
                        op.mark_lookups(call_stack, possibly_op + 1)
                    })
                });

                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, explore_queue, _|
                    {
                        let previous = explore_queue.clone();
                        *explore_queue = VecDeque::new();

                        call_stack.extend(args.iter().rev().map(|arg|
                        {
                            MarkStackEntry{
                                func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                                {
                                    arg.mark_lookups(
                                        call_stack,
                                        possibly_op + 1
                                    )
                                })
                            }
                        }));

                        *explore_queue = previous;

                        false
                    })
                });

                false
            },
            InterRepr::Sequence(sequence) =>
            {
                call_stack.extend(sequence.iter().rev().enumerate().map(|(index, value)|
                {
                    let is_last = index == 0;

                    MarkStackEntry{
                        func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                        {
                            value.mark_lookups(
                                call_stack,
                                if is_last { possibly_op } else { 0 }
                            )
                        })
                    }
                }));

                false
            },
            InterRepr::If{check, then, else_body} =>
            {
                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                    {
                        else_body.mark_lookups(call_stack, possibly_op)
                    })
                });

                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                    {
                        then.mark_lookups(call_stack, possibly_op)
                    })
                });

                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                    {
                        check.mark_lookups(call_stack, 0)
                    })
                });

                false
            },
            InterRepr::Define(DefineStage::Processed{name: _, position, body}) =>
            {
                call_stack.push(MarkStackEntry{
                    func: Box::new(move |call_stack, _interpret_state, _visited, lambda_defines, _explore_queue, _previous_skipped|
                    {
                        lambda_defines.insert(*position, LambdaDefineInfo{
                            last_possibly_op: 0,
                            last_lookup_position: None,
                            value: body
                        });

                        body.mark_lookups(call_stack, 0)
                    })
                });

                false
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{name: _, params: _, body} =>
            {
                if possibly_op > 0
                {
                    call_stack.push(MarkStackEntry{
                        func: Box::new(move |call_stack, _interpret_state, _visited, _lambda_defines, _explore_queue, _previous_skipped|
                        {
                            body.mark_lookups(call_stack, possibly_op.saturating_sub(1))
                        })
                    });

                    false
                } else
                {
                    true
                }
            },
            InterRepr::Quoted(_) => false,
            InterRepr::Lookup(LookupStage::Processed{symbol: id, pos}) =>
            {
                call_stack.push(MarkStackEntry{
                    func: Box::new(move |_call_stack, interpret_state, visited, lambda_defines, explore_queue, _previous_skipped|
                    {
                        let found_pos = if visited.contains(&pos.pos)
                        {
                            Self::compile_env_lookup_position(&interpret_state.compile_env, *id, pos)
                        } else
                        {
                            visited.push(pos.pos);
                            Self::compile_env_lookup(interpret_state, *id, pos).0
                        };

                        if possibly_op > 0
                        {
                            let found_pos = if let Some(x) = found_pos
                            {
                                x
                            } else
                            {
                                if interpret_state.outer_lookups.contains(id)
                                {
                                    if let Some(outer_position) = interpret_state.compile_env[0][0].iter().rposition(|(key, _)|
                                    {
                                        *id == *key
                                    }).map(|found_index|
                                    {
                                        EnvPosition{depth: 0, lambda_index: 0, index: found_index + 1}
                                    })
                                    {
                                        outer_position
                                    } else
                                    {
                                        return false;
                                    }
                                } else
                                {
                                    return false;
                                }
                            };

                            if let Some(this_lambda) = &mut lambda_defines.get_mut(&found_pos)
                            {
                                let last_possibly_op = this_lambda.last_possibly_op;
                                if possibly_op > last_possibly_op
                                {
                                    let same_call = this_lambda.last_lookup_position == Some(self.position);

                                    this_lambda.last_lookup_position = Some(self.position);
                                    this_lambda.last_possibly_op = possibly_op;

                                    explore_queue.push_back((possibly_op, this_lambda.value));

                                    !same_call
                                } else
                                {
                                    false
                                }
                            } else
                            {
                                false
                            }
                        } else
                        {
                            true
                        }
                    })
                });

                false
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => false
        }
    }

    fn process_lookups(&mut self, interpret_state: &mut InterpretState)
    {
        match &mut self.value
        {
            InterRepr::Apply{op, args} =>
            {
                interpret_state.with_new_env(|interpret_state|
                {
                    op.process_lookups(interpret_state);
                });

                args.iter_mut().for_each(|arg|
                {
                    interpret_state.with_new_env(|interpret_state|
                    {
                        arg.process_lookups(interpret_state);
                    });
                });
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.iter_mut().for_each(|value|
                {
                    value.process_lookups(interpret_state);
                });
            },
            InterRepr::If{check, then, else_body} =>
            {
                interpret_state.with_new_env(|interpret_state|
                {
                    check.process_lookups(interpret_state);
                });

                interpret_state.with_new_env(|interpret_state|
                {
                    then.process_lookups(interpret_state);
                });

                interpret_state.with_new_env(|interpret_state|
                {
                    else_body.process_lookups(interpret_state);
                });
            },
            InterRepr::Define(DefineStage::Parsed{name, body}) | InterRepr::Define(DefineStage::Processed{name, body, ..}) =>
            {
                {
                    let value = if let InterRepr::Value(x) = body.value
                    {
                        Some(x)
                    } else
                    {
                        None
                    };

                    let is_lambda = matches!(body.value, InterRepr::Lambda{..});

                    interpret_state.compile_env_add(*name, is_lambda, value)
                }

                self.value = InterRepr::Define(DefineStage::Processed{
                    name: *name,
                    body: body.clone(),
                    position: interpret_state.current_env_position.pos
                });

                if let InterRepr::Define(DefineStage::Processed{name: _, body, position: _}) = &mut self.value
                {
                    interpret_state.with_new_env(|interpret_state|
                    {
                        body.process_lookups(interpret_state);
                    });
                } else
                {
                    unreachable!()
                }
            },
            InterRepr::Lambda{name: _, params, body} =>
            {
                let restore_position = interpret_state.lambda_begin_env();

                params.add_to_compile_env(interpret_state);

                body.process_lookups(interpret_state);

                interpret_state.end_env(restore_position);
            },
            InterRepr::Quoted(_) => (),
            InterRepr::Lookup(LookupStage::Parsed{symbol: id}) | InterRepr::Lookup(LookupStage::Processed{symbol: id, ..}) =>
            {
                let pos = interpret_state.current_env_position.clone();

                self.value = Self::compile_env_lookup(interpret_state, *id, &pos).1.map(|value|
                {
                    InterRepr::Value(value)
                }).unwrap_or_else(||
                {
                    InterRepr::Lookup(LookupStage::Processed{symbol: *id, pos})
                });
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => ()
        }
    }

    fn has_side_effects(&self) -> bool
    {
        match &self.value
        {
            InterRepr::Apply{..} => true,
            InterRepr::Sequence(sequence) => sequence.iter().any(|x| x.has_side_effects()),
            InterRepr::If{check, then, else_body} => check.has_side_effects() || then.has_side_effects() || else_body.has_side_effects(),
            InterRepr::Define{..} => true,
            InterRepr::Lambda{..} => false,
            InterRepr::Quoted(_) => false,
            InterRepr::Lookup(_) => false,
            InterRepr::Value(_) => false
        }
    }

    fn is_control_flow_dependent(&self, apply_state: &ApplyState) -> bool
    {
        match &self.value
        {
            InterRepr::Apply{op, args} =>
            {
                if let InterRepr::Value(value) = op.value
                {
                    if let Ok(primitive_id) = value.as_primitive_procedure()
                    {
                        if let Some((effect, _)) = &apply_state.memory.primitives.get(primitive_id).on_apply
                        {
                            match effect
                            {
                                Effect::Pure | Effect::PureAllocating => (),
                                Effect::PureIf(condition) =>
                                {
                                    match condition
                                    {
                                        PureCondition::ArgsBetween{start, end_inclusive} =>
                                        {
                                            if !(*start..=*end_inclusive).contains(&args.len())
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                },
                                Effect::Impure => return true
                            }

                            for arg in args.iter().rev()
                            {
                                if arg.is_control_flow_dependent(apply_state)
                                {
                                    return true;
                                }
                            }

                            return false;
                        }
                    }

                    true
                } else if let InterRepr::Lambda{body, ..} = &op.value
                {
                    body.is_control_flow_dependent(apply_state)
                } else
                {
                    true
                }
            },
            InterRepr::Sequence(sequence) => sequence.iter().any(|x| x.is_control_flow_dependent(apply_state)),
            InterRepr::If{check, then, else_body} =>
            {
                check.is_control_flow_dependent(apply_state)
                    || then.is_control_flow_dependent(apply_state)
                    || else_body.is_control_flow_dependent(apply_state)
            },
            InterRepr::Define(DefineStage::Processed{body, ..}) =>
            {
                body.is_control_flow_dependent(apply_state)
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{..} => false,
            InterRepr::Quoted(_) => false,
            InterRepr::Lookup(LookupStage::Processed{symbol, pos}) =>
            {
                Self::compile_env_lookup_position(apply_state.compile_env, *symbol, pos).is_none()
                    && !apply_state.env_variables.contains(symbol)
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => false
        }
    }

    fn simple_replace(&mut self, params: &[WithPosition<SymbolId>], f: &mut impl FnMut(&mut Self, usize)) -> bool
    {
        match &mut self.value
        {
            InterRepr::Apply{op, args} =>
            {
                let is_op = op.simple_replace(params, f);

                is_op && args.iter_mut().all(|arg|
                {
                    arg.simple_replace(params, f)
                })
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.iter_mut().all(|value|
                {
                    value.simple_replace(params, f)
                })
            },
            InterRepr::If{check, then, else_body} =>
            {
                check.simple_replace(params, f) && then.simple_replace(params, f) && else_body.simple_replace(params, f)
            },
            InterRepr::Define(DefineStage::Processed{..}) =>
            {
                false
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{body, params: lambda_params, ..} =>
            {
                let lambda_params: &[_] = match lambda_params
                {
                    LambdaParams::Variadic(x) => &[*x],
                    LambdaParams::Normal(x) => &*x
                };

                let replace_params: Vec<_> = params.iter().filter(|param| !lambda_params.iter().any(|x| x.value == param.value)).copied().collect();

                body.simple_replace(&replace_params, f)
            },
            InterRepr::Quoted(_) => true,
            InterRepr::Lookup(LookupStage::Processed{symbol: id, pos: _}) =>
            {
                if let Some(index) = params.iter().position(|param| param.value == *id)
                {
                    f(self, index);
                }

                true
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => true
        }
    }

    fn fill_inline(&self, apply_state: &mut ApplyState)
    {
        match &self.value
        {
            InterRepr::Apply{op, args} =>
            {
                op.fill_inline(apply_state);

                args.iter().for_each(|arg|
                {
                    arg.fill_inline(apply_state);
                });
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.iter().for_each(|value|
                {
                    value.fill_inline(apply_state);
                });
            },
            InterRepr::If{check, then, else_body} =>
            {
                check.fill_inline(apply_state);
                then.fill_inline(apply_state);
                else_body.fill_inline(apply_state);
            },
            InterRepr::Define(DefineStage::Processed{position, body, ..}) =>
            {
                if apply_state.inline_lookup.as_ref().map(|x| x.position == *position).unwrap_or(false)
                {
                    let control_flow_dependent = body.is_control_flow_dependent(apply_state);

                    if !control_flow_dependent
                    {
                        apply_state.inline_lookup.as_mut().unwrap().value = Some(body.clone());
                        return;
                    }
                }

                body.fill_inline(apply_state);
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{name: _, params: _, body} =>
            {
                body.fill_inline(apply_state);
            },
            InterRepr::Quoted(_) => (),
            InterRepr::Lookup(_) => (),
            InterRepr::Value(_) => ()
        }
    }

    fn apply_known(
        &mut self,
        apply_state: &mut ApplyState
    )
    {
        match &mut self.value
        {
            InterRepr::Apply{op, args} =>
            {
                op.apply_known(apply_state);

                args.iter_mut().for_each(|arg|
                {
                    arg.apply_known(apply_state);
                });

                if let InterRepr::Value(value) = op.value
                {
                    if let Ok(primitive_id) = value.as_primitive_procedure()
                    {
                        if let Some((effect, f)) = &apply_state.memory.primitives.get(primitive_id).on_apply
                        {
                            match effect
                            {
                                Effect::Pure => (),
                                Effect::PureIf(condition) =>
                                {
                                    match condition
                                    {
                                        PureCondition::ArgsBetween{start, end_inclusive} =>
                                        {
                                            if !(*start..=*end_inclusive).contains(&args.len())
                                            {
                                                return;
                                            }
                                        }
                                    }
                                },
                                Effect::PureAllocating | Effect::Impure => return
                            }

                            let f = f.clone();

                            apply_state.memory.set_register(Register::Argument, ());

                            for arg in args.iter().rev()
                            {
                                let value = if let InterRepr::Value(value) = arg.value
                                {
                                    value
                                } else if let InterRepr::Quoted(AstPos{value: Ast::Value(value), ..}) = &arg.value
                                {
                                    Self::parse_primitive_text(apply_state.memory, value).unwrap()
                                } else
                                {
                                    return;
                                };

                                apply_state.memory.set_register(Register::Temporary, value);

                                if let Err(err) = apply_state.memory.cons(Register::Argument, Register::Temporary, Register::Argument)
                                {
                                    eprintln!("error in apply_known args: {err}");

                                    return;
                                }
                            }

                            match f(apply_state.memory, self.position, Register::Value)
                            {
                                Ok(_) =>
                                {
                                    apply_state.changed = true;
                                    self.value = InterRepr::Value(apply_state.memory.get_register(Register::Value))
                                },
                                Err(err) => eprintln!("error in apply_known: {err}")
                            }
                        }
                    }
                } else if let InterRepr::Lambda{params: LambdaParams::Normal(params), body, ..} = &mut op.value
                {
                    if args.len() != params.len()
                    {
                        return;
                    }

                    {
                        let mut counts = vec![0; params.len()];
                        if !body.simple_replace(params, &mut |_lookup_repr, index| counts[index] += 1)
                        {
                            return;
                        }

                        if !counts.into_iter().all(|x| x < 2)
                        {
                            return;
                        }
                    }

                    body.simple_replace(params, &mut |lookup_repr, index| *lookup_repr = args[index].clone());

                    apply_state.changed = true;
                    *self = *body.clone();
                }
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.iter_mut().for_each(|value|
                {
                    value.apply_known(apply_state);
                });

                let useless_head = sequence.iter().take(sequence.len().saturating_sub(1)).all(|x|
                {
                    !x.has_side_effects()
                });

                if sequence.is_empty()
                {
                    apply_state.changed = true;
                    self.value = InterRepr::Value(LispValue::new_empty_list());
                } else if useless_head || sequence.len() == 1
                {
                    apply_state.changed = true;
                    *self = sequence.iter().last().unwrap().clone();
                }
            },
            InterRepr::If{check, then, else_body} =>
            {
                check.apply_known(apply_state);

                let known_boolean = if let InterRepr::Value(value) = check.value
                {
                    value.as_bool().ok()
                } else
                {
                    None
                };

                if let Some(known_boolean) = known_boolean
                {
                    apply_state.changed = true;
                    *self = if known_boolean
                    {
                        *then.clone()
                    } else
                    {
                        *else_body.clone()
                    };

                    self.apply_known(apply_state);
                } else
                {
                    then.apply_known(apply_state);
                    else_body.apply_known(apply_state);
                }
            },
            InterRepr::Define(DefineStage::Processed{name: _, position: _, body}) =>
            {
                body.apply_known(apply_state);
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{name: _, params: _, body} =>
            {
                body.apply_known(apply_state);
            },
            InterRepr::Quoted(_) => (),
            InterRepr::Lookup(LookupStage::Processed{symbol, pos}) =>
            {
                if apply_state.inline_lookup.as_ref().map(|x| x.id == *symbol).unwrap_or(false)
                {
                    if let Some(found_position) = Self::compile_env_lookup_position(apply_state.compile_env, *symbol, pos)
                    {
                        if apply_state.inline_lookup.as_ref().map(|x| x.position == found_position).unwrap()
                        {
                            if let Some(inline_lookup) = apply_state.inline_lookup.take().unwrap().value.take()
                            {
                                apply_state.changed = true;
                                *self = *inline_lookup;
                            }
                        }
                    }
                }
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => ()
        }
    }

    fn parse_addresses(
        &mut self,
        interpret_state: &InterpretState
    )
    {
        match &mut self.value
        {
            InterRepr::Apply{op, args} =>
            {
                op.parse_addresses(interpret_state);

                args.iter_mut().for_each(|arg|
                {
                    arg.parse_addresses(interpret_state);
                });
            },
            InterRepr::Sequence(sequence) =>
            {
                sequence.iter_mut().for_each(|value|
                {
                    value.parse_addresses(interpret_state);
                });
            },
            InterRepr::If{check, then, else_body} =>
            {
                check.parse_addresses(interpret_state);
                then.parse_addresses(interpret_state);
                else_body.parse_addresses(interpret_state);
            },
            InterRepr::Define(DefineStage::Processed{name: _, position: _, body}) =>
            {
                body.parse_addresses(interpret_state);
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Lambda{name: _, params: _, body} =>
            {
                body.parse_addresses(interpret_state);
            },
            InterRepr::Quoted(_) => (),
            InterRepr::Lookup(LookupStage::Processed{symbol: id, pos: current_env_pos}) =>
            {
                let current_env_depth = current_env_pos.pos.depth;

                let found = iter::once(current_env_pos.pos)
                    .chain(current_env_pos.indices.iter().copied().rev())
                    .enumerate()
                    .find_map(|(nest_index, EnvPosition{depth: env_depth, lambda_index, index})|
                    {
                        let from_start = current_env_depth - env_depth;

                        let this_env = &interpret_state.compile_env[env_depth][lambda_index][..index];

                        this_env.iter().rposition(|(key, _)|
                        {
                            *id == *key
                        }).map(|symbol_index|
                        {
                            let skip_amount = this_env[..symbol_index].iter().filter(|x| x.1.mark_removed).count();

                            LexicalAddress{
                                up_env: from_start,
                                index: current_env_pos.offset(interpret_state, nest_index) + (symbol_index - skip_amount)
                            }
                        })
                    });

                self.value = InterRepr::Lookup(LookupStage::Final{symbol: *id, pos: current_env_pos.clone(), location: found});
            },
            InterRepr::Lookup(_) => unreachable!(),
            InterRepr::Value(_) => ()
        }
    }

    fn compile_allocated(
        memory: &mut LispMemory,
        target: Register,
        value: String
    ) -> CompiledPart
    {
        let get_primitive = |name|
        {
            LispValue::new_primitive_procedure(memory.primitives.index_by_name(name).unwrap())
        };

        let mut commands = vec![
            Command::PutValue{value: get_primitive(MAKE_VECTOR_PRIMITIVE), register: Register::Operator}.into(),
            Command::PutValue{value: LispValue::new_empty_list(), register: Register::Argument}.into(),
            Command::PutValue{value: LispValue::new_char(' '), register: Register::Value}.into(),
            Command::Cons{target: Register::Argument, car: Register::Value, cdr: Register::Argument}.into(),
            Command::PutValue{value: LispValue::new_integer(value.len() as i32), register: Register::Value}.into(),
            Command::Cons{target: Register::Argument, car: Register::Value, cdr: Register::Argument}.into(),
            Command::CallPrimitiveValueUnchecked{target: Register::Temporary}.into()
        ];

        value.chars().enumerate().for_each(|(index, c)|
        {
            commands.extend([
                Command::PutValue{value: get_primitive(VECTOR_SET_PRIMITIVE), register: Register::Operator}.into(),
                Command::PutValue{value: LispValue::new_empty_list(), register: Register::Argument}.into(),
                Command::PutValue{value: LispValue::new_char(c), register: Register::Value}.into(),
                Command::Cons{target: Register::Argument, car: Register::Value, cdr: Register::Argument}.into(),
                Command::PutValue{value: LispValue::new_integer(index as i32), register: Register::Value}.into(),
                Command::Cons{target: Register::Argument, car: Register::Value, cdr: Register::Argument}.into(),
                Command::Cons{target: Register::Argument, car: Register::Temporary, cdr: Register::Argument}.into(),
                Command::CallPrimitiveValueUnchecked{target: Register::Value}.into()
            ]);
        });

        commands.extend([
            Command::PutValue{value: memory.new_symbol("string"), register: Register::Value}.into(),
            Command::Cons{target, car: Register::Value, cdr: Register::Temporary}.into()
        ]);

        CompiledPart::from_commands(commands)
    }

    fn compile_quoted(
        memory: &mut LispMemory,
        target: Register,
        ast: AstPos
    ) -> CompiledPart
    {
        match ast.value
        {
            Ast::Value(x) =>
            {
                let value = Self::parse_primitive_text(memory, &x).unwrap();

                Command::PutValue{value, register: target}.into()
            },
            Ast::Allocated(x) =>
            {
                Self::compile_allocated(memory, target, x)
            },
            Ast::EmptyList =>
            {
                Command::PutValue{value: LispValue::new_empty_list(), register: target}.into()
            },
            Ast::List{car, cdr} =>
            {
                let car = Self::compile_quoted(memory, Register::Value, *car);
                let cdr = Self::compile_quoted(memory, Register::Temporary, *cdr);

                let cons = CompiledPart::from(Command::Cons{target, car: Register::Value, cdr: Register::Temporary})
                    .with_requires(RegisterStates::one(Register::Value));

                car.combine(cdr.combine_preserving(cons, RegisterStates::one(Register::Value)))
            }
        }
    }

    fn compile_cons(
        state: &mut CompileState,
        position: CodePosition,
        target: Register,
        args: Vec<Self>
    ) -> CompiledPart
    {
        if args.len() != 2
        {
            return CompiledPart::from(Command::Error(Error::WrongArgumentsCount{
                proc: CONS_PRIMITIVE.to_owned(),
                expected: "2".to_owned(),
                got: args.len()
            }.with_position(position)));
        }

        let mut args = args.into_iter();

        let other_register = if target == Register::Argument
        {
            Register::Value
        } else
        {
            Register::Argument
        };

        let car_part = args.next().unwrap().compile(state, Some(target), Proceed::Next);
        let cdr_part = args.next().unwrap().compile(state, Some(other_register), Proceed::Next);

        let op_part = CompiledPart::from(Command::Cons{target, car: target, cdr: other_register})
            .with_requires(RegisterStates::one(other_register));

        let call_part = car_part.combine_preserving(op_part, RegisterStates::one(other_register));

        cdr_part.combine_preserving(call_part, RegisterStates::one(Register::Environment).set(Register::Return))
    }

    fn compile_car(
        state: &mut CompileState,
        position: CodePosition,
        target: Register,
        args: Vec<Self>
    ) -> CompiledPart
    {
        if args.len() != 1
        {
            return CompiledPart::from(Command::Error(Error::WrongArgumentsCount{
                proc: CAR_PRIMITIVE.to_owned(),
                expected: "1".to_owned(),
                got: args.len()
            }.with_position(position)));
        }

        let arg_part = args.into_iter().next().unwrap().compile(state, Some(target), Proceed::Next);
        let op_part = CompiledPart::from(Command::Car{target, source: target});

        arg_part.combine(op_part)
    }

    fn compile_cdr(
        state: &mut CompileState,
        position: CodePosition,
        target: Register,
        args: Vec<Self>
    ) -> CompiledPart
    {
        if args.len() != 1
        {
            return CompiledPart::from(Command::Error(Error::WrongArgumentsCount{
                proc: CDR_PRIMITIVE.to_owned(),
                expected: "1".to_owned(),
                got: args.len()
            }.with_position(position)));
        }

        let arg_part = args.into_iter().next().unwrap().compile(state, Some(target), Proceed::Next);
        let op_part = CompiledPart::from(Command::Cdr{target, source: target});

        arg_part.combine(op_part)
    }

    fn compile(
        self,
        state: &mut CompileState,
        target: PutValue,
        proceed: Proceed
    ) -> CompiledPart
    {
        match self.value
        {
            InterRepr::Value(value) =>
            {
                if let Some(register) = target
                {
                    CompiledPart::from_commands(vec![Command::PutValue{value, register}.with_position(self.position)])
                } else
                {
                    CompiledPart::new()
                }.with_proceed(proceed)
            },
            InterRepr::Lookup(LookupStage::Final{symbol: id, pos: _, location}) =>
            {
                if let Some(register) = target
                {
                    let command = location.map(|location| Command::Lookup{location, register})
                        .unwrap_or(Command::LookupOuter{id, register});

                    CompiledPart::from_commands(vec![command.with_position(self.position)])
                        .with_requires(RegisterStates::one(Register::Environment))
                } else
                {
                    CompiledPart::new()
                }.with_proceed(proceed)
            },
            InterRepr::Lookup(x) => unreachable!("{x:#?}"),
            InterRepr::Sequence(values) =>
            {
                let len = values.len();
                values.into_iter().enumerate().map(|(i, x)|
                {
                    if (i + 1) == len
                    {
                        x.compile(state, target, proceed)
                    } else
                    {
                        x.compile(state, None, Proceed::Next)
                    }
                }).reduce(|acc, x|
                {
                    acc.combine_preserving(x, RegisterStates::one(Register::Environment).set(Register::Return))
                }).unwrap_or_else(CompiledPart::new)
            },
            InterRepr::If{check, then, else_body} =>
            {
                let else_branch = Label::ElseBranch(state.label_id());

                let check_pos = check.position;
                let check_value = check.compile(state, Some(Register::Value), Proceed::Next);

                let type_check = if state.type_checks
                {
                    let post_branch = Label::AfterError(state.label_id());

                    CompiledPart::from_commands(vec![
                        Command::IsTag{check: Register::Value, tag: ValueTag::Bool}.into(),
                        Command::JumpIfTrue{target: post_branch, check: Register::Temporary}.into(),
                        Command::Error(Error::WrongConditionalType(String::new()).with_position(check_pos)).into(),
                        Command::Label(post_branch).into()
                    ])
                } else
                {
                    CompiledPart::new()
                };

                let check = check_value.combine(type_check);

                let after_if_label = Label::AfterIf(state.label_id());

                let then_proceed = match proceed
                {
                    Proceed::Next => Proceed::Jump(after_if_label),
                    x => x
                };

                let then_part = CompiledPart::from(Command::JumpIfFalse{target: else_branch, check: Register::Value})
                    .combine(then.compile(state, target, then_proceed));

                let else_part = CompiledPart::from(Command::Label(else_branch))
                    .combine(else_body.compile(state, target, proceed));

                let if_body = check.combine_preserving(
                    then_part.combine(else_part),
                    RegisterStates::one(Register::Environment).set(Register::Return)
                );

                if let Proceed::Next = proceed
                {
                    if_body.combine(Command::Label(after_if_label))
                } else
                {
                    if_body
                }
            },
            InterRepr::Lambda{name, params, body} =>
            {
                let target = if let Some(target) = target
                {
                    target
                } else
                {
                    return CompiledPart::new();
                };

                let params_define = params.compile(state, name);
                let body = body.compile(state, Some(Register::Value), Proceed::Return);
                let label = state.add_lambda(params_define.combine(body));

                let label_part: CompiledPart = Command::PutLabel{target, label}.into();

                let cons_part = CompiledPart::from_commands(vec![
                    Command::Cons{target, car: Register::Environment, cdr: target}.with_position(self.position)
                ]);

                let lambda = label_part.combine(cons_part).with_requires(RegisterStates::one(Register::Environment));

                lambda.with_proceed(proceed)
            },
            InterRepr::Define(DefineStage::Processed{name, position: _, body}) =>
            {
                let temp = if let Some(target) = target
                {
                    target
                } else
                {
                    Register::Value
                };

                let mut commands = vec![Command::Define{id: name, register: temp}.with_position(self.position)];

                if let Some(target) = target
                {
                    commands.push(Command::PutValue{value: ().into(), register: target}.into());
                }

                let body = body.compile(state, Some(temp), Proceed::Next);
                body.combine_preserving(
                    CompiledPart::from_commands(commands).with_requires(RegisterStates::one(Register::Environment)),
                    RegisterStates::one(Register::Environment)
                ).with_proceed(proceed)
            },
            InterRepr::Define(_) => unreachable!(),
            InterRepr::Quoted(ast) =>
            {
                if let Some(register) = target
                {
                    Self::compile_quoted(state.memory, register, ast)
                } else
                {
                    CompiledPart::new()
                }.with_proceed(proceed)
            },
            InterRepr::Apply{op, args} =>
            {
                let is_known_primitive = op.is_known_primitive();
                let is_known_compound = op.is_known_compound(state);

                if state.apply_known && !state.type_checks
                {
                    if let InterRepr::Value(value) = &op.value
                    {
                        if let Ok(p) = value.as_primitive_procedure()
                        {
                            type FuncType = fn(&mut CompileState, CodePosition, Register, Vec<InterReprPos>) -> CompiledPart;

                            if let Some(f) = if p == state.cons_symbol
                            {
                                Some::<FuncType>(Self::compile_cons)
                            } else if p == state.car_symbol
                            {
                                Some::<FuncType>(Self::compile_car)
                            } else if p == state.cdr_symbol
                            {
                                Some::<FuncType>(Self::compile_cdr)
                            } else
                            {
                                None
                            }
                            {
                                if let Some(target) = target
                                {
                                    return f(state, op.position, target, args).with_proceed(proceed);
                                } else
                                {
                                    return CompiledPart::new().with_proceed(proceed)
                                }
                            }
                        }
                    }
                }

                let args_count = args.len();

                let empty_list: CompiledPart = Command::PutValue{
                    value: LispValue::new_empty_list(),
                    register: Register::Argument
                }.into();

                let args_part = args.into_iter().rev().fold(empty_list, |acc, x|
                {
                    let ending: CommandPos = Command::Cons{
                        target: Register::Argument,
                        car: Register::Value,
                        cdr: Register::Argument
                    }.with_position(self.position);

                    let ending = CompiledPart::from(ending)
                        .with_requires(RegisterStates::one(Register::Argument));

                    let body = x.compile(state, Some(Register::Value), Proceed::Next)
                        .combine_preserving(ending, RegisterStates::one(Register::Argument));

                    acc.combine_preserving(body, RegisterStates::one(Register::Environment).set(Register::Return))
                });

                let operator_setup = op.compile(state, Some(Register::Operator), Proceed::Next);

                let primitive_return: CompiledPart = match proceed
                {
                    Proceed::Jump(label) => Command::Jump(label).into(),
                    Proceed::Next => CompiledPart::new(),
                    Proceed::Return =>
                    {
                        CompiledPart::from(Command::JumpRegister(Register::Return))
                            .with_requires(RegisterStates::one(Register::Return))
                    }
                };

                let primitive_branch = Label::PrimitiveBranch(state.label_id());
                let check_part = CompiledPart::from_commands(vec![
                    Command::IsTag{check: Register::Operator, tag: ValueTag::PrimitiveProcedure}.into(),
                    Command::JumpIfTrue{target: primitive_branch, check: Register::Temporary}.into()
                ]);

                let compound_call_part = CompiledPart::from_commands(vec![
                    Command::Cdr{target: Register::Operator, source: Register::Operator}.into(),
                    Command::JumpRegister(Register::Operator).into()
                ]).with_modifies(RegisterStates::all());

                let (compound_check, compound_error) = if state.type_checks
                {
                    let error_branch = Label::ErrorBranch(state.label_id());

                    (CompiledPart::from_commands(vec![
                        Command::IsTag{check: Register::Operator, tag: ValueTag::List}.into(),
                        Command::JumpIfFalse{target: error_branch, check: Register::Temporary}.into(),
                        Command::Car{target: Register::Environment, source: Register::Operator}.into(),
                        Command::IsTag{check: Register::Environment, tag: ValueTag::List}.into(),
                        Command::JumpIfFalse{target: error_branch, check: Register::Temporary}.into(),
                        Command::Car{target: Register::Temporary, source: Register::Environment}.into(),
                        Command::IsTag{check: Register::Temporary, tag: ValueTag::EnvironmentMarker}.into(),
                        Command::JumpIfFalse{target: error_branch, check: Register::Temporary}.into()
                    ]), CompiledPart::from_commands(vec![
                        Command::Label(error_branch).into(),
                        Command::Error(Error::CallNonProcedure{got: String::new()}.with_position(self.position)).into()
                    ]))
                } else
                {
                    (CompiledPart::from_commands(vec![
                        Command::Car{target: Register::Environment, source: Register::Operator}.into()
                    ]), CompiledPart::new())
                };

                let env_part = CompiledPart::from_commands(vec![
                    Command::CreateChildEnvironment.with_position(self.position)
                ]).with_requires(RegisterStates::one(Register::Environment));

                let compound_part_basic = compound_check
                    .combine(env_part)
                    .combine(compound_call_part)
                    .combine(compound_error);

                let after_procedure = Label::AfterProcedure(state.label_id());

                let compound_part = if target.is_none() || target == Some(Register::Value)
                {
                    let prepare_return = match proceed
                    {
                        Proceed::Jump(label) => Command::PutLabel{target: Register::Return, label}.into(),
                        Proceed::Next => Command::PutLabel{target: Register::Return, label: after_procedure}.into(),
                        Proceed::Return => CompiledPart::new().with_requires(RegisterStates::one(Register::Return))
                    };

                    prepare_return.combine(compound_part_basic)
                } else
                {
                    let procedure_return = Label::ProcedureReturn(state.label_id());

                    let prepare_return: CompiledPart = Command::PutLabel{
                        target: Register::Return,
                        label: procedure_return
                    }.into();

                    let proceed = match proceed
                    {
                        Proceed::Next =>
                        {
                            if !is_known_compound
                            {
                                Command::Jump(after_procedure).into()
                            } else
                            {
                                CompiledPart::new()
                            }
                        },
                        _ => proceed.into_compiled()
                    };

                    prepare_return.combine(compound_part_basic).combine(CompiledPart::from_commands(vec![
                        Command::Label(procedure_return).into(),
                        Command::Move{target: target.expect("checked in branch"), source: Register::Value}.into()
                    ])).combine_preserving(proceed, RegisterStates::one(Register::Return))
                };

                let primitive_commands = if state.type_checks
                {
                    vec![
                        Command::PutValue{value: LispValue::new_length(args_count as u32), register: Register::Temporary}.into(),
                        Command::CallPrimitiveValue{target: target.unwrap_or(Register::Temporary)}
                            .with_position(self.position)
                    ]
                } else
                {
                    vec![
                        Command::CallPrimitiveValueUnchecked{target: target.unwrap_or(Register::Temporary)}
                            .with_position(self.position)
                    ]
                };

                let primitive_part = CompiledPart::from_commands(primitive_commands)
                    .combine_preserving(primitive_return, RegisterStates::one(Register::Return));

                let call_part = if is_known_compound
                {
                    compound_part
                } else
                {
                    if is_known_primitive
                    {
                        primitive_part
                    } else
                    {
                        check_part.combine(compound_part)
                            .combine(Command::Label(primitive_branch))
                            .combine(primitive_part)
                    }
                };

                let after_procedure = if proceed == Proceed::Next && !is_known_primitive
                {
                    CompiledPart::from(Command::Label(after_procedure))
                } else
                {
                    CompiledPart::new()
                };

                let call_with_return = call_part.combine(after_procedure)
                    .with_requires(RegisterStates::one(Register::Operator));

                let after_operator = args_part.combine_preserving(
                    call_with_return,
                    RegisterStates::one(Register::Operator).set(Register::Environment).set(Register::Return)
                );

                operator_setup.combine_preserving(
                    after_operator,
                    RegisterStates::one(Register::Environment).set(Register::Return)
                )
            }
        }
    }

    fn print_debug(&self, memory: &LispMemory, static_indent: usize, indent: usize)
    {
        let print_indent = ||
        {
            eprint!("{}", str::repeat("  ", indent));
        };

        match &self.value
        {
            InterRepr::Apply{op, args} =>
            {
                print_indent();
                eprint!("(");

                op.print_debug(memory, static_indent, 0);

                args.iter().for_each(|arg|
                {
                    eprint!(" ");

                    arg.print_debug(memory, static_indent, 0);
                });

                eprint!(")");
            },
            InterRepr::Sequence(sequence) =>
            {
                print_indent();
                eprintln!("(begin");

                sequence.iter().enumerate().for_each(|(index, x)|
                {
                    x.print_debug(memory, static_indent + 1, static_indent + 1);

                    if (index + 1) != sequence.len()
                    {
                        eprintln!();
                    }
                });

                eprint!(")");
            },
            InterRepr::If{check, then, else_body} =>
            {
                print_indent();
                eprint!("(if ");

                check.print_debug(memory, static_indent, 0);
                eprintln!();

                then.print_debug(memory, static_indent + 1, static_indent + 1);
                eprintln!();

                else_body.print_debug(memory, static_indent + 1, static_indent + 1);

                eprint!(")");
            },
            InterRepr::Define(DefineStage::Parsed{body, name, ..})
            | InterRepr::Define(DefineStage::Processed{body, name, ..}) =>
            {
                print_indent();

                let name = memory.get_symbol(*name);

                if let InterRepr::Lambda{params: LambdaParams::Normal(symbols), body, ..} = &body.value
                {
                    eprintln!(
                        "(define ({})",
                        iter::once(name)
                            .chain(symbols.iter().map(|symbol| memory.get_symbol(**symbol)))
                            .reduce(|acc, x| acc + " " + &x).unwrap_or_default()
                    );

                    body.print_debug(memory, static_indent + 1, static_indent + 1);
                } else
                {
                    eprintln!("(define {name}");

                    body.print_debug(memory, static_indent + 1, static_indent + 1);
                }

                eprint!(")");
            },
            InterRepr::Lambda{params, body, ..} =>
            {
                let params = match params
                {
                    LambdaParams::Variadic(symbol) => memory.get_symbol(**symbol),
                    LambdaParams::Normal(symbols) =>
                    {
                        format!("({})", symbols.iter().map(|symbol| memory.get_symbol(**symbol)).reduce(|acc, x| acc + " " + &x).unwrap_or_default())
                    }
                };

                print_indent();
                eprintln!("(lambda {params}");

                body.print_debug(memory, static_indent + 1, static_indent + 1);

                eprint!(")");
            },
            InterRepr::Quoted(ast) =>
            {
                print_indent();
                eprint!("'{}", ast.to_string_pretty());
            },
            InterRepr::Lookup(LookupStage::Parsed{symbol, ..})
            | InterRepr::Lookup(LookupStage::Processed{symbol, ..})
            | InterRepr::Lookup(LookupStage::Final{symbol, ..}) =>
            {
                print_indent();
                eprint!("{}", memory.get_symbol(*symbol));
            },
            InterRepr::Value(value) =>
            {
                print_indent();

                let s = if let Ok(p) = value.as_primitive_procedure()
                {
                    memory.primitives.name_by_index(p).to_owned()
                } else
                {
                    value.to_string(memory)
                };

                eprint!("{s}");
            }
        }
    }
}

impl InterRepr
{
    fn parse_if(
        interpret_state: &mut InterpretState,
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let args = InterReprPos::parse_args(interpret_state, memory, ast)?;

        let mut args = args.into_iter();

        let check = Box::new(args.next().unwrap());

        let then_body = Box::new(args.next().unwrap());

        let else_body = Box::new(args.next().unwrap_or_else(||
        {
            Self::Value(LispValue::new_empty_list()).with_position(then_body.position)
        }));

        Ok(Self::If{check, then: then_body, else_body})
    }

    fn parse_let(
        interpret_state: &mut InterpretState,
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let position = ast.position;

        let params_ast = ast.car();

        let mut params = Vec::new();
        let mut args = Vec::new();

        params_ast.list_to_vec().into_iter().try_for_each(|pair|
        {
            if !pair.is_list() || pair.is_null()
            {
                return Err(ErrorPos{position: pair.position, value: Error::LetNoValue});
            }

            let name = pair.car();

            let rest = pair.cdr();

            if rest.is_null()
            {
                return Err(ErrorPos{position: rest.position, value: Error::LetNoValue});
            }

            let arg = rest.car();

            let last = rest.cdr();
            if !last.is_null()
            {
                return Err(ErrorPos{position: last.position, value: Error::LetTooMany});
            }

            let param = WithPosition{
                position: pair.position,
                value: InterReprPos::parse_symbol(memory, &name)?
            };

            params.push(param);

            args.push(InterReprPos::parse(interpret_state, memory, arg)?);

            Ok(())
        })?;

        let params = LambdaParams::Normal(params);

        let body = InterReprPos::parse_args(interpret_state, memory, ast.cdr())?
            .into_iter().next().unwrap();

        let lambda = Self::Lambda{name: "<lambda>".to_owned(), params, body: Box::new(body)}
            .with_position(position);

        Ok(Self::Apply{op: Box::new(lambda), args})
    }

    fn parse_lambda(
        interpret_state: &mut InterpretState,
        memory: &mut LispMemory,
        name: String,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let params = ast.car();

        let cdr = ast.cdr();

        let bodies_position = cdr.position;

        let params = LambdaParams::parse(memory, params)?;

        let bodies = InterReprPos::parse_args(interpret_state, memory, cdr)?;
        let body = InterRepr::Sequence(bodies).with_position(bodies_position);

        Ok(Self::Lambda{name, params, body: Box::new(body)})
    }
}

#[derive(Debug, Clone)]
struct LambdaDefineInfo<'a>
{
    last_possibly_op: u32,
    last_lookup_position: Option<CodePosition>,
    value: &'a InterReprPos
}

#[derive(Debug, Clone, Copy)]
pub struct LexicalAddress
{
    pub up_env: usize,
    pub index: usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnvPosition
{
    depth: usize,
    lambda_index: usize,
    index: usize
}

#[derive(Debug, Clone)]
pub struct CompileEnvPosition
{
    pos: EnvPosition,
    indices: Vec<EnvPosition>
}

impl Default for CompileEnvPosition
{
    fn default() -> Self
    {
        Self{
            pos: EnvPosition{
                depth: 0,
                lambda_index: 0,
                index: 0
            },
            indices: Vec::new()
        }
    }
}

impl CompileEnvPosition
{
    fn offset(&self, interpret_state: &InterpretState, nest_index: usize) -> usize
    {
        let current_pos = iter::once(self.pos).chain(self.indices.iter().copied().rev()).nth(nest_index);

        let first_depth = if let Some(x) = current_pos
        {
            x.depth
        } else
        {
            return 0;
        };

        let env = &interpret_state.compile_env[first_depth];

        let mut offset = 0;
        for x in self.indices.iter().copied().rev().skip(nest_index)
        {
            if x.depth != first_depth
            {
                break;
            }

            offset += env[x.lambda_index].iter().filter(|x| !x.1.mark_removed).count();
        }

        offset
    }
}

#[allow(dead_code)]
struct InterpretStateDebugWithSymbols<'a>
{
    symbols: Option<&'a Symbols>,
    state: &'a InterpretState
}

impl Debug for InterpretStateDebugWithSymbols<'_>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let outer_lookups: Vec<_> = self.state.outer_lookups.iter().map(|x|
        {
            DebugRaw(self.symbols.map(|symbols| symbols.get_by_id(*x).to_owned()).unwrap_or_else(|| x.to_string()))
        }).collect();

        let compile_env: Vec<_> = self.state.compile_env.iter().map(|x| -> Vec<_>
        {
            x.iter().map(|x| -> Vec<_>
            {
                x.iter().map(|(key, value)|
                {
                    let name = self.symbols.map(|symbols| symbols.get_by_id(*key).to_owned()).unwrap_or_else(||
                    {
                        key.to_string()
                    });

                    let CompileSymbolState{looked_up, mark_removed, value, ..} = value;

                    let used_count: usize = *looked_up as usize;

                    let x = if let Some(value) = value
                    {
                        format!("{name} -> {value:?}")
                    } else
                    {
                        name
                    };

                    let encountered = if used_count > 0
                    {
                        format!("{used_count} ")
                    } else
                    {
                        "(UNUSED) ".to_owned()
                    };

                    let needs_removal = if *mark_removed { "TO-REMOVE " } else { "" };

                    DebugRaw(needs_removal.to_owned() + &encountered + &x)
                }).collect()
            }).collect()
        }).collect();

        f.debug_struct("InterpretState")
            .field("eval_encountered", &self.state.eval_encountered)
            .field("outer_lookups", &outer_lookups)
            .field("current_env_position", &self.state.current_env_position)
            .field("compile_env", &compile_env)
            .finish()
    }
}

#[derive(Debug)]
pub struct CompileSymbolState
{
    looked_up: u8,
    mark_removed: bool,
    is_lambda: bool,
    value: Option<LispValue>
}

pub struct InterpretState
{
    eval_encountered: bool,
    outer_lookups: Vec<SymbolId>,
    current_env_position: CompileEnvPosition,
    compile_env: Vec<Vec<Vec<(SymbolId, CompileSymbolState)>>>,
    load_handler: Option<(SymbolId, usize, Box<dyn Fn(&str) -> Option<String>>)>,
    debug_mode_symbol: SymbolId,
    debug_mode: bool,
    #[cfg(debug_assertions)]
    defined_symbols: Option<Symbols>
}

impl Debug for InterpretState
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        #[cfg(debug_assertions)]
        {
            return <InterpretStateDebugWithSymbols as Debug>::fmt(&InterpretStateDebugWithSymbols{
                symbols: self.defined_symbols.as_ref(),
                state: self
            }, f);
        }

        #[allow(unreachable_code)]
        <InterpretStateDebugWithSymbols as Debug>::fmt(&InterpretStateDebugWithSymbols{
            symbols: None,
            state: self
        }, f)
    }
}

impl InterpretState
{
    fn reset_compile_env(&mut self)
    {
        self.current_env_position = CompileEnvPosition::default();
        self.outer_lookups.clear();
        self.compile_env = vec![vec![Vec::new()]];
    }

    fn clear_compile_env_lookups(&mut self)
    {
        self.current_env_position = CompileEnvPosition::default();

        self.outer_lookups.clear();

        self.compile_env.iter_mut().for_each(|depth_lookups|
        {
            depth_lookups.iter_mut().for_each(|lambda_lookups|
            {
                lambda_lookups.iter_mut().for_each(|(_id, lookup)| lookup.looked_up = 0);
            });
        });
    }

    fn compile_env_add(&mut self, name: SymbolId, is_lambda: bool, value: Option<LispValue>)
    {
        let p = self.current_env_position.pos;

        let this_env = &mut self.compile_env[p.depth][p.lambda_index];

        this_env.push((name, CompileSymbolState{looked_up: 0, mark_removed: false, is_lambda, value}));
        self.current_env_position.pos.index = this_env.len();
    }

    fn with_new_env<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T
    {
        let restore_position = self.new_begin_env();

        let output = f(self);

        self.end_env(restore_position);

        output
    }

    #[must_use = "env position must be restored"]
    fn new_begin_env(&mut self) -> CompileEnvPosition
    {
        let previous_env_position = self.current_env_position.clone();

        self.current_env_position.indices.push(self.current_env_position.pos);
        self.compile_env[self.current_env_position.pos.depth].push(Vec::new());

        self.current_env_position.pos.lambda_index = self.compile_env[self.current_env_position.pos.depth].len() - 1;
        self.current_env_position.pos.index = 0;

        previous_env_position
    }

    fn end_env(&mut self, previous_position: CompileEnvPosition)
    {
        self.current_env_position = previous_position;
    }

    #[must_use = "env position must be restored"]
    fn lambda_begin_env(&mut self) -> CompileEnvPosition
    {
        let previous_env_position = self.current_env_position.clone();

        self.current_env_position.indices.push(self.current_env_position.pos);
        self.current_env_position.pos.depth += 1;

        if self.current_env_position.pos.depth == self.compile_env.len()
        {
            self.compile_env.push(vec![Vec::new()]);
        } else
        {
            self.compile_env[self.current_env_position.pos.depth].push(Vec::new());
        }

        self.current_env_position.pos.lambda_index = self.compile_env[self.current_env_position.pos.depth].len() - 1;
        self.current_env_position.pos.index = 0;

        previous_env_position
    }
}

#[derive(Debug)]
struct InlineLookup
{
    id: SymbolId,
    position: EnvPosition,
    value: Option<Box<InterReprPos>>
}

#[derive(Debug)]
struct ApplyState<'a, 'b, 'c>
{
    inline_lookup: Option<InlineLookup>,
    compile_env: &'a [Vec<Vec<(SymbolId, CompileSymbolState)>>],
    env_variables: &'a Vec<SymbolId>,
    discard_inlines: &'b mut Vec<EnvPosition>,
    memory: &'c mut LispMemory,
    changed: bool
}

#[derive(Debug)]
struct CompileState<'a, 'b>
{
    pub memory: &'a mut LispMemory,
    compile_env: &'b [Vec<Vec<(SymbolId, CompileSymbolState)>>],
    type_checks: bool,
    apply_known: bool,
    lambdas: Vec<CompiledPart>,
    cons_symbol: u32,
    car_symbol: u32,
    cdr_symbol: u32,
    label_id: u32
}

impl<'a, 'b> CompileState<'a, 'b>
{
    pub fn new(
        memory: &'a mut LispMemory,
        compile_env: &'b [Vec<Vec<(SymbolId, CompileSymbolState)>>],
        type_checks: bool,
        apply_known: bool
    ) -> Self
    {
        let get_symbol = |name: &str| memory.primitives.index_by_name(name).unwrap();

        let cons_symbol = get_symbol(CONS_PRIMITIVE);
        let car_symbol = get_symbol(CAR_PRIMITIVE);
        let cdr_symbol = get_symbol(CDR_PRIMITIVE);

        Self{
            memory,
            compile_env,
            type_checks,
            apply_known,
            lambdas: Vec::new(),
            cons_symbol,
            car_symbol,
            cdr_symbol,
            label_id: 0
        }
    }

    pub fn label_id(&mut self) -> u32
    {
        let id = self.label_id;

        self.label_id += 1;

        id
    }

    pub fn add_lambda(&mut self, lambda: CompiledPart) -> Label
    {
        let id = self.lambdas.len();
        let label = Label::Procedure(id as u32);

        self.lambdas.push(CompiledPart::from(Command::Label(label)).combine(lambda));

        label
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumCount, FromRepr)]
pub enum Register
{
    Return,
    Environment,
    Operator,
    Argument,
    Value,
    Temporary
}

pub type PutValue = Option<Register>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Label
{
    Halt,
    ErrorBranch(u32),
    AfterError(u32),
    ElseBranch(u32),
    AfterIf(u32),
    Procedure(u32),
    AfterProcedure(u32),
    PrimitiveBranch(u32),
    ProcedureReturn(u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proceed
{
    Next,
    Jump(Label),
    Return
}

impl Proceed
{
    fn into_compiled(self) -> CompiledPart
    {
        match self
        {
            Self::Next => CompiledPart::new(),
            Self::Jump(label) => CompiledPart::from(Command::Jump(label)),
            Self::Return =>
            {
                CompiledPart::from(Command::JumpRegister(Register::Return))
                    .with_requires(RegisterStates::one(Register::Return))
            }
        }
    }
}

#[derive(Debug, Clone)]
enum CommandRaw
{
    Push(Register),
    Pop(Register),
    PutValue{value: LispValue, register: Register},
    Move{target: Register, source: Register},
    Lookup{location: LexicalAddress, register: Register},
    LookupOuter{id: SymbolId, register: Register},
    Define{id: SymbolId, register: Register},
    CreateChildEnvironment,
    Jump(usize),
    JumpRegister(Register),
    JumpIfTrue{target: usize, check: Register},
    JumpIfFalse{target: usize, check: Register},
    IsTag{check: Register, tag: ValueTag},
    Cons{target: Register, car: Register, cdr: Register},
    Car{target: Register, source: Register},
    Cdr{target: Register, source: Register},
    Error(ErrorPos),
    CallPrimitiveValue{target: Register},
    CallPrimitiveValueUnchecked{target: Register}
}

struct CommandRawDisplay<'a>
{
    memory: &'a LispMemory,
    value: &'a CommandRaw
}

impl Debug for CommandRawDisplay<'_>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self.value
        {
            CommandRaw::PutValue{value, register} =>
            {
                f.debug_struct("PutValue")
                    .field("value", &DebugRaw(value.to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            CommandRaw::LookupOuter{id, register} =>
            {
                f.debug_struct("LookupOuter")
                    .field("id", &DebugRaw(LispValue::new_symbol_raw(*id).to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            CommandRaw::Define{id, register} =>
            {
                f.debug_struct("Define")
                    .field("id", &DebugRaw(LispValue::new_symbol_raw(*id).to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            x => write!(f, "{x:?}")
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RegisterStates([bool; Register::COUNT]);

impl Index<Register> for RegisterStates
{
    type Output = bool;

    fn index(&self, register: Register) -> &Self::Output
    {
        &self.0[register as usize]
    }
}

impl IndexMut<Register> for RegisterStates
{
    fn index_mut(&mut self, register: Register) -> &mut Self::Output
    {
        &mut self.0[register as usize]
    }
}

impl Default for RegisterStates
{
    fn default() -> Self
    {
        Self::none()
    }
}

impl RegisterStates
{
    pub fn all() -> Self
    {
        Self([true; Register::COUNT])
    }

    pub fn none() -> Self
    {
        Self([false; Register::COUNT])
    }

    pub fn one(register: Register) -> Self
    {
        Self::none().set(register)
    }

    pub fn set(mut self, register: Register) -> Self
    {
        self[register] = true;

        self
    }

    pub fn into_iter(self) -> impl DoubleEndedIterator<Item=(Register, bool)> + Clone
    {
        self.0.into_iter().enumerate().map(|(index, x)| (Register::from_repr(index).unwrap(), x))
    }

    pub fn zip_map(self, other: Self, f: impl FnMut((bool, bool)) -> bool) -> Self
    {
        Self(self.0.into_iter().zip(other.0).map(f).collect::<Vec<_>>().try_into().unwrap())
    }

    pub fn intersection(self, other: Self) -> Self
    {
        self.zip_map(other, |(a, b)|
        {
            a && b
        })
    }

    pub fn union(self, other: Self) -> Self
    {
        self.zip_map(other, |(a, b)|
        {
            a || b
        })
    }

    pub fn difference(self, other: Self) -> Self
    {
        self.zip_map(other, |(a, b)|
        {
            a && !b
        })
    }
}

#[derive(Debug, Clone)]
pub struct CompiledProgram
{
    positions: Vec<Option<CodePosition>>,
    commands: Vec<CommandRaw>
}

impl CompiledProgram
{
    #[cfg(test)]
    pub fn commands_count(&self) -> usize
    {
        self.commands.len()
    }

    #[cfg(test)]
    pub fn commands_lookup_outer_count(&self) -> usize
    {
        self.commands.iter().filter(|x|
        {
            if let CommandRaw::LookupOuter{..} = x
            {
                true
            } else
            {
                false
            }
        }).count()
    }

    #[cfg(test)]
    pub fn commands_define_count(&self) -> usize
    {
        self.commands.iter().filter(|x|
        {
            if let CommandRaw::Define{..} = x
            {
                true
            } else
            {
                false
            }
        }).count()
    }

    fn run(&self, memory: &mut LispMemory) -> Result<(), ErrorPos>
    {
        let mut i = 0;
        while i < self.commands.len()
        {
            macro_rules! code_position
            {
                ($name:literal) =>
                {
                    self.positions[i].unwrap_or_else(|| panic!("{} must have a codepos", $name))
                }
            }

            macro_rules! return_error
            {
                ($error:expr, $name:literal) =>
                {
                    return Err(ErrorPos{
                        position: code_position!($name),
                        value: $error
                    })
                }
            }

            let command = &self.commands[i];

            if DebugConfig::is_enabled(DebugTool::Lisp)
            {
                eprintln!("[RUNNING] {i}: {:?}", CommandRawDisplay{memory, value: command});
            }

            match command
            {
                CommandRaw::Push(register) =>
                {
                    if let Err(err) = memory.push_stack_register(*register)
                    {
                        return Err(ErrorPos{
                            position: CodePosition::default(),
                            value: err
                        })
                    }
                },
                CommandRaw::Pop(register) =>
                {
                    memory.pop_stack_register(*register);
                },
                CommandRaw::Lookup{location, register} =>
                {
                    memory.set_register(*register, memory.lookup_location(*location));
                },
                CommandRaw::LookupOuter{id, register} =>
                {
                    if let Some(value) = memory.lookup_symbol_outer(*id)
                    {
                        memory.set_register(*register, value);
                    } else
                    {
                        return_error!(Error::UndefinedVariable(memory.get_symbol(*id)), "lookup")
                    }
                },
                CommandRaw::Define{id, register} =>
                {
                    if let Err(err) = memory.define_symbol(*id, *register)
                    {
                        return_error!(err, "define")
                    }
                },
                CommandRaw::CreateChildEnvironment =>
                {
                    let parent = memory.get_register(Register::Environment);
                    match memory.create_env(parent)
                    {
                        Ok(env) => memory.set_register(Register::Environment, env),
                        Err(err) => return_error!(err, "create child environment")
                    }
                },
                CommandRaw::PutValue{value, register} => memory.set_register(*register, *value),
                CommandRaw::Move{target, source} =>
                {
                    let value = memory.get_register(*source);

                    memory.set_register(*target, value);
                },
                CommandRaw::Jump(destination) =>
                {
                    i = *destination;
                    continue;
                },
                CommandRaw::JumpRegister(register) =>
                {
                    i = memory.get_register(*register).as_address().expect("must be checked") as usize;
                    continue;
                },
                CommandRaw::JumpIfTrue{target, check} =>
                {
                    if memory.get_register(*check).as_bool().expect("must be checked")
                    {
                        i = *target;
                        continue;
                    }
                },
                CommandRaw::JumpIfFalse{target, check} =>
                {
                    if !memory.get_register(*check).as_bool().expect("must be checked")
                    {
                        i = *target;
                        continue;
                    }
                },
                CommandRaw::IsTag{check, tag} =>
                {
                    let is_primitive = memory.get_register(*check).tag == *tag;
                    memory.set_register(Register::Temporary, is_primitive);
                },
                CommandRaw::Cons{target, car, cdr} =>
                {
                    if let Err(err) = memory.cons(*target, *car, *cdr)
                    {
                        return_error!(err, "cons")
                    }
                },
                CommandRaw::Car{target, source} =>
                {
                    let value = memory.get_car(memory.get_register(*source).as_list_id().expect("must be a list"));

                    memory.set_register(*target, value);
                },
                CommandRaw::Cdr{target, source} =>
                {
                    let value = memory.get_cdr(memory.get_register(*source).as_list_id().expect("must be a list"));

                    memory.set_register(*target, value);
                },
                CommandRaw::Error(err) =>
                {
                    let mut err = err.clone();
                    match &mut err.value
                    {
                        Error::CallNonProcedure{got} =>
                        {
                            *got = memory.get_register(Register::Operator).to_string(memory);
                        },
                        Error::WrongConditionalType(s) =>
                        {
                            *s = memory.get_register(Register::Value).to_string(memory);
                        },
                        Error::WrongArgumentsCount{got, ..} =>
                        {
                            let mut leftover = 0;

                            while let Ok(lst) = memory.get_register(Register::Argument).as_list(memory)
                            {
                                memory.set_register(Register::Argument, lst.cdr);

                                leftover += 1;
                            }

                            *got += leftover;
                        },
                        _ => ()
                    }

                    return Err(err);
                },
                CommandRaw::CallPrimitiveValue{target}
                | CommandRaw::CallPrimitiveValueUnchecked{target} =>
                {
                    let op = memory.get_register(Register::Operator).as_primitive_procedure()
                        .expect("must be checked");

                    let primitive = memory.primitives.get(op);

                    if let CommandRaw::CallPrimitiveValue{..} = command
                    {
                        let count = memory.get_register(Register::Temporary).as_length()
                            .expect("must be set");

                        if !primitive.args_count.contains(count as usize)
                        {
                            return_error!(Error::WrongArgumentsCount{
                                proc: memory.primitives.name_by_index(op).to_owned(),
                                expected: primitive.args_count.to_string(),
                                got: count as usize
                            }, "primitive")
                        }
                    }

                    let primitive = primitive.on_apply.as_ref()
                        .expect("primitive must have apply")
                        .1
                        .clone();

                    if let Err(err) = primitive(memory, self.positions[i].unwrap_or_default(), *target)
                    {
                        return_error!(err, "primitive")
                    }
                }
            }

            i += 1;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct CompileConfig
{
    pub type_checks: bool,
    pub apply_known: bool
}

impl Default for CompileConfig
{
    fn default() -> Self
    {
        Self{
            type_checks: cfg!(debug_assertions) && DebugConfig::is_disabled(DebugTool::LispDisableChecks),
            apply_known: !cfg!(test)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program
{
    memory: LispMemory,
    code: CompiledProgram
}

impl Program
{
    pub fn parse(
        LispConfig{
            compile_config: config,
            load_handler,
            env_variables,
            mut memory
        }: LispConfig,
        code: &[&str]
    ) -> Result<Self, ErrorPos>
    {
        debug_assert!(memory.iter_values().all(|x| x.tag != ValueTag::Address));

        let ast = Parser::parse(0, code)?;

        let mut interpret_state = InterpretState{
            eval_encountered: false,
            outer_lookups: Vec::new(),
            current_env_position: CompileEnvPosition::default(),
            compile_env: vec![vec![Vec::new()]],
            load_handler: load_handler.map(|x|
            {
                let load_symbol = memory.new_symbol_id("load");
                (load_symbol, code.len(), x)
            }),
            debug_mode_symbol: memory.new_symbol_id("debug-mode"),
            debug_mode: config.type_checks,
            #[cfg(debug_assertions)]
            defined_symbols: None
        };

        let mut ir = InterReprPos::parse(
            &mut interpret_state,
            &mut memory,
            ast
        )?;

        #[cfg(debug_assertions)]
        {
            interpret_state.defined_symbols = Some(memory.symbols());
        }

        ir.process_lookups(&mut interpret_state);

        if config.apply_known
        {
            let env_variables = env_variables.into_iter().map(|x| memory.new_symbol_id(&x)).collect();
            let mut discard_inlines = Vec::new();

            loop
            {
                let mut apply_state = ApplyState{
                    inline_lookup: interpret_state.compile_env.iter().enumerate().find_map(|(depth, depth_env)|
                    {
                        let interpret_state = &interpret_state;
                        let discard_inlines = &mut *discard_inlines;
                        depth_env.iter().enumerate().find_map(move |(lambda_index, lambda_env)|
                        {
                            let outer_lookups = &interpret_state.outer_lookups;
                            let discard_inlines = &mut *discard_inlines;

                            lambda_env.iter().enumerate().find_map(move |(index, (name, info))|
                            {
                                if info.looked_up == 1 && !outer_lookups.contains(name)
                                {
                                    let maybe_found = InlineLookup{
                                        id: *name,
                                        position: EnvPosition{depth, lambda_index, index: index + 1},
                                        value: None
                                    };

                                    if !discard_inlines.contains(&maybe_found.position)
                                    {
                                        return Some(maybe_found);
                                    }
                                }

                                None
                            })
                        })
                    }),
                    compile_env: &interpret_state.compile_env,
                    env_variables: &env_variables,
                    discard_inlines: &mut discard_inlines,
                    memory: &mut memory,
                    changed: false
                };

                if apply_state.inline_lookup.is_some()
                {
                    ir.fill_inline(&mut apply_state);

                    if apply_state.inline_lookup.as_ref().map(|x| x.value.is_none()).unwrap()
                    {
                        apply_state.discard_inlines.push(apply_state.inline_lookup.as_ref().unwrap().position);
                        continue;
                    }
                }

                ir.apply_known(&mut apply_state);

                let changed = apply_state.changed;

                interpret_state.reset_compile_env();
                ir.process_lookups(&mut interpret_state);

                if !interpret_state.eval_encountered
                {
                    interpret_state.clear_compile_env_lookups();

                    {
                        let mut call_stack = Vec::new();

                        let mut visited = Vec::new();
                        let mut lambda_defines = HashMap::new();
                        let mut explore_queue = VecDeque::new();

                        let mut last_any_skipped = ir.mark_lookups(&mut call_stack, 0);

                        while let Some(next_call) = call_stack.pop()
                        {
                            last_any_skipped = (next_call.func)(
                                &mut call_stack,
                                &mut interpret_state,
                                &mut visited,
                                &mut lambda_defines,
                                &mut explore_queue,
                                last_any_skipped
                            );
                        }
                    }

                    ir.remove_unused_defines(&mut interpret_state);
                }

                if !changed
                {
                    break;
                }
            }
        }

        ir.parse_addresses(&interpret_state);

        if DebugConfig::is_enabled(DebugTool::Lisp)
        {
            ir.print_debug(&memory, 0, 0);
            eprintln!();
        }

        let code = {
            let mut state = CompileState::new(&mut memory, &interpret_state.compile_env, config.type_checks, config.apply_known);

            let compiled = ir.compile(&mut state, Some(Register::Value), Proceed::Jump(Label::Halt));

            compiled.into_program(state)
        }?;

        Ok(Self{memory, code})
    }

    pub fn eval(&self, with_memory: impl FnOnce(&mut LispMemory)) -> Result<OutputWrapper, ErrorPos>
    {
        let mut memory = self.memory.clone();

        with_memory(&mut memory);
        self.code.run(&mut memory)?;

        let value = memory.get_register(Register::Value);
        Ok(OutputWrapper{memory, value})
    }

    pub fn eval_mut(&mut self) -> Result<OutputWrapperRef<'_>, ErrorPos>
    {
        self.memory.clear();

        self.eval_precleared()
    }

    pub fn eval_precleared(&mut self) -> Result<OutputWrapperRef<'_>, ErrorPos>
    {
        self.code.run(&mut self.memory)?;

        let value = self.memory.get_register(Register::Value);
        Ok(OutputWrapperRef{memory: &self.memory, value})
    }

    pub fn memory_mut(&mut self) -> &mut LispMemory
    {
        &mut self.memory
    }

    #[cfg(test)]
    pub fn code(&self) -> &CompiledProgram
    {
        &self.code
    }
}
