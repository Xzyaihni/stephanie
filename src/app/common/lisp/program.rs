use std::{
    vec,
    iter,
    array,
    borrow::Borrow,
    iter::{Map, Enumerate},
    rc::Rc,
    cell::RefCell,
    io::{self, Write},
    fmt::{self, Debug, Display},
    collections::HashMap,
    ops::{RangeInclusive, Add, Sub, Mul, Div, Rem, Deref, Index, IndexMut}
};

use strum::{EnumCount, FromRepr};

use crate::debug_config::*;

pub use super::{
    transfer_with_capacity,
    Error,
    ErrorPos,
    SymbolId,
    LispValue,
    LispMemory,
    ValueTag,
    LispVectorRef,
    OutputWrapper,
    OutputWrapperRef
};

pub use parser::{PrimitiveType, CodePosition, WithPosition, WithPositionMaybe, WithPositionTrait};

use parser::{Parser, Ast, AstPos};

mod parser;


pub const BEGIN_PRIMITIVE: &'static str = "begin";
pub const QUOTE_PRIMITIVE: &'static str = "quote";
pub const CONS_PRIMITIVE: &'static str = "cons";

// unreadable, great
pub type OnApply = Rc<
    dyn Fn(
        &mut LispMemory,
        Register
    ) -> Result<(), Error>>;

pub type OnEval = Rc<
    dyn Fn(
        &mut LispMemory,
        &Primitives,
        AstPos
    ) -> Result<InterRepr, ErrorPos>>;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect
{
    Pure,
    Impure
}

impl<T> WithPositionTrait<Result<T, ErrorPos>> for Result<T, Error>
{
    fn with_position(self, position: CodePosition) -> Result<T, ErrorPos>
    {
        self.map_err(|error| ErrorPos{position, error})
    }
}

pub struct PrimitiveArgs<'a>
{
    memory: &'a mut LispMemory
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
    Rc::new(move |memory, target|
    {
        let value = f(PrimitiveArgs{memory})?;
        memory.set_register(target, value);

        Ok(())
    })
}

#[derive(Clone)]
pub struct PrimitiveProcedureInfo
{
    args_count: ArgsCount,
    on_eval: Option<OnEval>,
    on_apply: Option<(Effect, OnApply)>
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
}

impl Debug for PrimitiveProcedureInfo
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "<procedure with {} args>", &self.args_count)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HiddenPrimitive
{
    Add,
    Sub,
    Mul,
    Div
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrimitiveName
{
    String(String),
    Hidden(HiddenPrimitive)
}

impl From<HiddenPrimitive> for PrimitiveName
{
    fn from(value: HiddenPrimitive) -> Self
    {
        Self::Hidden(value)
    }
}

impl From<String> for PrimitiveName
{
    fn from(value: String) -> Self
    {
        Self::String(value)
    }
}

impl From<&str> for PrimitiveName
{
    fn from(value: &str) -> Self
    {
        Self::from(value.to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct Primitives
{
    indices: HashMap<PrimitiveName, u32>,
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
                |args|
                {
                    Self::do_cond(memory, |a, b| Some($f(a, b)), |a, b| Some($f(a, b)))
                }
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
            ($tag:expr) =>
            {
                |mut args|
                {
                    let is_equal = args.next().unwrap().tag == $tag;

                    Ok(is_equal.into())
                }
            }
        }

        let (indices, primitives): (HashMap<PrimitiveName, _>, Vec<_>) = [
            (PrimitiveName::from(BEGIN_PRIMITIVE),
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(1), Rc::new(|memory, primitives, args|
                {
                    Ok(InterRepr::Sequence(InterReprPos::parse_args(memory, primitives, args)?))
                }))),
            (QUOTE_PRIMITIVE.into(),
                PrimitiveProcedureInfo::new_eval(1, Rc::new(|memory, _primitives, args|
                {
                    Ok(InterRepr::Value(InterReprPos::parse_quote_args(memory, args)?))
                }))),
            (CONS_PRIMITIVE.into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |args|
                {
                    todo!()
                    // memory.cons(target, args[0], args[1])
                })),
            ("+".into(), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(add, checked_add))),
        ]/*[
            ("display",
                PrimitiveProcedureInfo::new_simple_effect(1, move |_state, memory, mut args|
                {
                    let arg = args.pop(memory);

                    print!("{arg}");
                    io::stdout().flush().unwrap();

                    memory.push_return(());

                    Ok(())
                })),
            ("newline",
                PrimitiveProcedureInfo::new_simple_effect(0, move |_state, memory, _args|
                {
                    println!();

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

                    memory.allocate_vector(vec)
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
                            .ok_or(Error::IndexOutOfRange(index))? = value.value;

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

                    let value = vec.try_get(index as usize).ok_or(Error::IndexOutOfRange(index))?;
                    memory.push_return(value);

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
                    } else
                    {
                        let number = arg.as_integer()?;

                        memory.push_return(number as f32);
                    }

                    Ok(())
                })),
            ("inexact->exact",
                PrimitiveProcedureInfo::new_simple(1, |_state, memory, mut args|
                {
                    let arg = *args.pop(memory);

                    if arg.tag == ValueTag::Integer
                    {
                        memory.push_return(arg);
                    } else
                    {
                        let number = arg.as_float()?;

                        memory.push_return(number.round() as i32);
                    }

                    Ok(())
                })),
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
                    Rc::new(|eval_queue, _state, _memory, args, action|
                    {
                        let has_else = args.1.len() == 3;
                        let args = &args.0;

                        eval_queue.push(Evaluated{
                            args: EvaluatedArgs{
                                expr: Some(args.cdr()),
                                args: None
                            },
                            run: Box::new(move |EvaluatedArgs{expr, ..}, eval_queue, _state, memory|
                            {
                                let args = expr.unwrap();

                                memory.restore_env();

                                let predicate = memory.pop_return();

                                let on_true = args.car();

                                let predicate = predicate.is_true();

                                if predicate
                                {
                                    on_true.eval(eval_queue, memory, action)
                                } else
                                {
                                    #[allow(clippy::collapsible_else_if)]
                                    if has_else
                                    {
                                        let on_false = args.cdr().car();

                                        on_false.eval(eval_queue, memory, action)
                                    } else
                                    {
                                        if action == Action::Return
                                        {
                                            memory.push_return(());
                                        }

                                        Ok(())
                                    }
                                }
                            })
                        });

                        eval_queue.push(Evaluated{
                            args: EvaluatedArgs{
                                expr: Some(args.car()),
                                args: None
                            },
                            run: Box::new(move |EvaluatedArgs{expr, ..}, eval_queue, _state, memory|
                            {
                                memory.save_env();

                                expr.unwrap().eval(eval_queue, memory, Action::Return)
                            })
                        });

                        Ok(())
                    }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, memory, args|
                {
                    let bindings = args.car();
                    let body = args.cdr().car();

                    let params = bindings.map_list(|x| x.car());
                    let apply_args = ExpressionPos::analyze_args(
                        state,
                        memory,
                        bindings.map_list(|x| x.cdr().car())
                    )?;

                    let lambda_args =
                        AstPos::cons(
                            params,
                            AstPos::cons(
                                body,
                                Ast::EmptyList.with_position(args.position)));

                    let lambda = ExpressionPos::analyze_lambda(state, memory, lambda_args)?;

                    Ok(ExpressionPos{
                        position: args.position,
                        expression: Expression::Application{
                            op: Box::new(lambda),
                            args: apply_args
                        }
                    })
                }))),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|_op, state, memory, args|
                {
                    ExpressionPos::analyze_lambda(state, memory, args)
                }))),
            ("define",
                PrimitiveProcedureInfo::new(ArgsCount::Min(2), Rc::new(|op, state, memory, mut args|
                {
                    if args.is_null()
                    {
                        return Err(ErrorPos{position: args.position, error: Error::ExpectedArg});
                    }

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
                                    ast: Ast::Value(BEGIN_PRIMITIVE.to_owned())
                                },
                                body
                            )
                        } else
                        {
                            body.into_iter().next().unwrap()
                        };

                        let name = ExpressionPos::analyze(state, memory, first.car())?;
                        let name = Expression::Value(name.as_value()?).with_position(position);

                        let params = first.cdr();

                        let lambda_args =
                            AstPos::cons(
                                params,
                                AstPos::cons(
                                    body,
                                    Ast::EmptyList.with_position(position)));

                        let lambda = ExpressionPos::analyze_lambda(state, memory, lambda_args)?;

                        let args = ExpressionPos::cons(
                            name,
                            ExpressionPos::cons(
                                lambda,
                                Expression::EmptyList.with_position(position)));

                        (Box::new(args), ArgsWrapper::from(2))
                    } else
                    {
                        ExpressionPos::analyze_args(state, memory, args)?
                    };

                    Ok(ExpressionPos{
                        position: args.0.position,
                        expression: Expression::Application{
                            op: Box::new(op),
                            args
                        }
                    })
                }), Rc::new(|eval_queue, _state, _memory, args, action|
                {
                    let first = args.0.car();
                    let second = args.0.cdr().car();

                    let pos = first.position;

                    let key = first.as_value()?;

                    eval_queue.push(Evaluated{
                        args: EvaluatedArgs{
                            expr: None,
                            args: None
                        },
                        run: Box::new(move |EvaluatedArgs{..}, _eval_queue, _state, memory|
                        {
                            memory.restore_env();

                            let value = memory.pop_return();

                            memory.define_symbol(key, value).with_position(pos)?;

                            if action == Action::Return
                            {
                                memory.push_return(());
                            }

                            Ok(())
                        })
                    });

                    eval_queue.push(Evaluated{
                        args: EvaluatedArgs{
                            expr: Some(second),
                            args: None
                        },
                        run: Box::new(move |EvaluatedArgs{expr: second, ..}, eval_queue, _state, memory|
                        {
                            memory.save_env();

                            second.unwrap().eval(eval_queue, memory, Action::Return)
                        })
                    });

                    Ok(())
                }))),
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
        ]*/.into_iter().enumerate().map(|(index, (k, v))|
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
        self.indices.insert(PrimitiveName::String(name), id as u32);
    }

    pub fn name_by_index(&self, index: u32) -> &PrimitiveName
    {
        self.indices.iter().find(|(_key, value)|
        {
            **value == index
        }).expect("index must exist").0
    }

    pub fn index_by_name(&self, name: impl Into<String>) -> Option<u32>
    {
        self.index_by_primitive_name(PrimitiveName::String(name.into()))
    }

    pub fn index_by_primitive_name(&self, primitive_name: impl Borrow<PrimitiveName>) -> Option<u32>
    {
        self.indices.get(primitive_name.borrow()).copied()
    }

    pub fn get_by_name(&self, name: String) -> Option<&PrimitiveProcedureInfo>
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
        op_integer: FI,
        op_float: FF
    ) -> Result<(), Error>
    where
        FI: Fn(i32, i32) -> Option<LispValue>,
        FF: Fn(f32, f32) -> Option<LispValue>
    {
        /*let first = *args.pop(memory);
        let second = *args.pop(memory);

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        let is_true = output.as_bool()?;

        if !is_true || args.is_empty()
        {
            args.clear(memory);

            memory.push_stack(output);

            Ok(())
        } else
        {
            args.push(memory, second);

            Self::do_cond(memory, op_integer, op_float)
        }*/
        todo!()
    }
}

#[derive(Debug, Clone)]
enum Command
{
    Push(Register),
    Pop(Register),
    PutRegister{target: Register, source: Register},
    PutValue{value: LispValue, register: Register},
    Lookup{id: SymbolId, register: Register},
    PutReturn(Label),
    Jump(Label),
    JumpIfTrue{target: Label, check: Register},
    IsPrimitiveProcedure,
    Cons{target: Register, car: Register, cdr: Register},
    CallPrimitiveValue{target: Register},
    Label(Label)
}

impl Command
{
    pub fn modifies_register(&self) -> Option<Register>
    {
        match self
        {
            Self::PutRegister{target: register, ..}
            | Self::PutValue{register, ..}
            | Self::Lookup{register, ..}
            | Self::Cons{target: register, ..}
            | Self::CallPrimitiveValue{target: register, ..} => Some(*register),
            Self::IsPrimitiveProcedure => Some(Register::Temporary),
            Self::PutReturn(_) => Some(Register::Return),
            Self::Push(_)
            | Self::Pop(_)
            | Self::Jump(_)
            | Self::JumpIfTrue{..}
            | Self::Label(_) => None
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
            Self::PutRegister{target, source} => CommandRaw::PutRegister{target, source},
            Self::PutValue{value, register} => CommandRaw::PutValue{value, register},
            Self::Lookup{id, register} => CommandRaw::Lookup{id, register},
            Self::PutReturn(label) =>
            {
                CommandRaw::PutValue{
                    value: LispValue::new_address(*labels.get(&label).unwrap() as u32),
                    register: Register::Return
                }
            },
            Self::Jump(label) => CommandRaw::Jump(*labels.get(&label).unwrap()),
            Self::JumpIfTrue{target, check} => CommandRaw::JumpIfTrue{target: *labels.get(&target).unwrap(), check},
            Self::IsPrimitiveProcedure => CommandRaw::IsPrimitiveProcedure,
            Self::Cons{target, car, cdr} => CommandRaw::Cons{target, car, cdr},
            Self::CallPrimitiveValue{target} => CommandRaw::CallPrimitiveValue{target},
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
        let mut modifies = RegisterStates::default();
        commands.iter().for_each(|CommandPos{value, ..}|
        {
            if let Some(register) = value.modifies_register()
            {
                modifies[register] = true;
            }
        });

        Self{
            modifies,
            requires: RegisterStates::default(),
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
        let other = other.into();

        let save = other.requires.intersection(self.modifies);

        let save_registers = save.into_iter().filter(|(_, x)| *x);

        let commands = save_registers.clone().map(|(register, _)| -> CommandPos
            {
                Command::Push(register).into()
            })
            .chain(self.commands.into_iter())
            .chain(save_registers.rev().map(|(register, _)| -> CommandPos
            {
                Command::Pop(register).into()
            }))
            .chain(other.commands)
            .collect();

        Self{
            modifies: self.modifies.union(other.modifies),
            requires: self.requires,
            commands
        }
    }

    pub fn simple_add(mut self, commands: impl IntoIterator<Item=CommandPos>) -> Self
    {
        self.commands.extend(commands.into_iter());

        self
    }

    pub fn into_program(mut self, primitives: Rc<Primitives>) -> CompiledProgram
    {
        self.commands.push(Command::Label(Label::Halt).into());

        if DebugConfig::is_enabled(DebugTool::Lisp)
        {
            self.commands.iter().for_each(|WithPositionMaybe{value, position}|
            {
                eprintln!("{value:?}{}", position.map(|x| format!(" ({x})")).unwrap_or_default());
            });
        }

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

        let (positions, commands) = self.commands.into_iter().filter(|command|
        {
            !command.is_label()
        }).map(|WithPositionMaybe{position, value: command}|
        {
            (position, command.into_raw(&labels))
        }).unzip();

        CompiledProgram{
            primitives,
            positions,
            commands
        }
    }
}

pub type InterReprPos = WithPosition<InterRepr>;

#[derive(Debug)]
pub enum InterRepr
{
    Apply{op: Box<InterReprPos>, args: Vec<InterReprPos>},
    Sequence(Vec<InterReprPos>),
    Lookup(SymbolId),
    Value(LispValue)
}

impl InterReprPos
{
    pub fn parse(
        memory: &mut LispMemory,
        primitives: &Primitives,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        match ast.value
        {
            Ast::Value(x) =>
            {
                let p: Result<_, ErrorPos> = Ast::parse_primitive(&x).with_position(ast.position);
                let value = memory.new_primitive_value(p?);

                Ok(if let Ok(id) = value.as_symbol_id()
                {
                    if let Some(primitive_id) = primitives.index_by_name(&memory.get_symbol(id))
                    {
                        InterRepr::Value(LispValue::new_primitive_procedure(primitive_id))
                    } else
                    {
                        InterRepr::Lookup(id)
                    }
                } else
                {
                    InterRepr::Value(value)
                }.with_position(ast.position))
            },
            Ast::EmptyList => Ok(InterRepr::Value(LispValue::new_empty_list()).with_position(ast.position)),
            Ast::List{car, cdr} =>
            {
                let op = Self::parse(memory, primitives, *car)?;

                if let InterRepr::Value(value) = op.value
                {
                    if let Ok(id) = value.as_primitive_procedure()
                    {
                        if let Some(on_eval) = &primitives.get(id).on_eval
                        {
                            return on_eval(memory, primitives, *cdr).map(|x| x.with_position(ast.position));
                        }
                    }
                }

                let args = Self::parse_args(memory, primitives, *cdr)?;

                Ok(InterRepr::Apply{op: Box::new(op), args}.with_position(ast.position))
            }
        }
    }

    pub fn parse_quote_args(
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<LispValue, ErrorPos>
    {
        let value = if let Ast::List{car, cdr} = ast.value
        {
            if cdr.is_null()
            {
                *car
            } else
            {
                return Err(Error::WrongArgumentsCount{
                    proc: QUOTE_PRIMITIVE.to_owned(),
                    this_invoked: false,
                    expected: "1".to_owned(),
                    got: None
                }).with_position(ast.position);
            }
        } else
        {
            unreachable!()
        };

        dbg!(&value);
        Self::allocate_quote(memory, value, Register::Value)?;

        let value = memory.get_register(Register::Value);
        memory.add_quoted(value);

        Ok(value)
    }

    fn allocate_quote(
        memory: &mut LispMemory,
        ast: AstPos,
        target: Register
    ) -> Result<(), ErrorPos>
    {
        let value = match ast.value
        {
            Ast::Value(x) =>
            {
                let p: Result<_, ErrorPos> = Ast::parse_primitive(&x).with_position(ast.position);
                memory.new_primitive_value(p?)
            },
            Ast::EmptyList => LispValue::new_empty_list(),
            Ast::List{car, cdr} =>
            {
                memory.push_stack_register(Register::Temporary);
                Self::allocate_quote(memory, *car, Register::Temporary)?;
                Self::allocate_quote(memory, *cdr, Register::Value)?;

                let result = memory.cons(target, Register::Temporary, Register::Value).with_position(ast.position);

                memory.pop_stack_register(Register::Temporary);

                return result;
            }
        };

        memory.set_register(target, value);

        Ok(())
    }

    pub fn parse_args(
        memory: &mut LispMemory,
        primitives: &Primitives,
        ast: AstPos
    ) -> Result<Vec<Self>, ErrorPos>
    {
        match ast.value
        {
            Ast::Value(_) => unreachable!("malformed ast"),
            Ast::EmptyList => Ok(Vec::new()),
            Ast::List{car, cdr} =>
            {
                let tail = Self::parse_args(memory, primitives, *cdr)?;

                Ok(iter::once(Self::parse(memory, primitives, *car)?).chain(tail).collect())
            }
        }
    }

    pub fn compile(
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
                }.combine(proceed.into_compiled())
            },
            InterRepr::Lookup(id) =>
            {
                if let Some(register) = target
                {
                    CompiledPart::from_commands(vec![Command::Lookup{id, register}.with_position(self.position)])
                } else
                {
                    CompiledPart::new()
                }.combine(proceed.into_compiled())
            },
            InterRepr::Sequence(values) =>
            {
                let len = values.len();
                values.into_iter().rev().enumerate().map(|(i, x)|
                {
                    if (i + 1) == len
                    {
                        x.compile(state, target, proceed)
                    } else
                    {
                        x.compile(state, None, Proceed::Next)
                    }
                }).reduce(CompiledPart::combine).unwrap_or_else(CompiledPart::new)
            },
            InterRepr::Apply{op, args} =>
            {
                let empty_list: CompiledPart = Command::PutValue{
                    value: LispValue::new_empty_list(),
                    register: Register::Argument
                }.into();

                let args_part = args.into_iter().rev().map(|x|
                {
                    x.compile(state, Some(Register::Argument), Proceed::Next)
                }).fold(empty_list, |acc, x|
                {
                    let separator: CompiledPart = Command::PutRegister{
                        target: Register::Temporary,
                        source: Register::Argument
                    }.into();

                    let ending: CommandPos = Command::Cons{
                        target: Register::Argument,
                        car: Register::Argument,
                        cdr: Register::Temporary
                    }.with_position(self.position);

                    acc.combine(separator).combine(x).combine(ending)
                });

                let setup = op.compile(state, Some(Register::Operator), Proceed::Next).combine(args_part);

                let return_command = match proceed
                {
                    Proceed::Jump(label) => Command::PutReturn(label),
                    Proceed::Next => Command::PutReturn(todo!()),
                    Proceed::Return => Command::Pop(Register::Return)
                };

                let make_lambda_branch_not_halt = ();

                let primitive_branch = Label::PrimitiveBranch(state.label_id());
                let call_part = CompiledPart::from_commands(vec![
                    return_command.into(),
                    Command::IsPrimitiveProcedure.into(),
                    Command::JumpIfTrue{target: primitive_branch, check: Register::Temporary}.with_position(self.position),
                    Command::Jump(Label::Halt).into(),
                    Command::Label(primitive_branch).into(),
                    Command::CallPrimitiveValue{target: target.expect("make None target be ok later")}.into()
                ]);

                setup.combine(call_part.with_modifies(RegisterStates::all()))
            }
        }
    }
}

#[derive(Debug)]
struct CompileState
{
    label_id: u32
}

impl CompileState
{
    pub fn new(primitives: &Primitives) -> Self
    {
        Self{
            label_id: 0
        }
    }

    pub fn label_id(&mut self) -> u32
    {
        let id = self.label_id;

        self.label_id += 1;

        id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumCount, FromRepr)]
pub enum Register
{
    Return,
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
            Self::Jump(label) => CompiledPart::from_commands(vec![Command::Jump(label).into()]),
            Self::Return => todo!()
        }
    }
}

#[derive(Debug, Clone)]
enum CommandRaw
{
    Push(Register),
    Pop(Register),
    PutRegister{target: Register, source: Register},
    PutValue{value: LispValue, register: Register},
    Lookup{id: SymbolId, register: Register},
    Jump(usize),
    JumpIfTrue{target: usize, check: Register},
    IsPrimitiveProcedure,
    Cons{target: Register, car: Register, cdr: Register},
    CallPrimitiveValue{target: Register}
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
}

#[derive(Debug, Clone)]
pub struct CompiledProgram
{
    primitives: Rc<Primitives>,
    positions: Vec<Option<CodePosition>>,
    commands: Vec<CommandRaw>
}

impl CompiledProgram
{
    fn run(&self, memory: &mut LispMemory) -> Result<(), ErrorPos>
    {
        let mut i = 0;
        while i < self.commands.len()
        {
            macro_rules! return_error
            {
                ($error:expr, $name:literal) =>
                {
                    return Err(ErrorPos{
                        position: self.positions[i].unwrap_or_else(|| panic!("{} must have a codepos", $name)),
                        error: $error
                    })
                }
            }

            if DebugConfig::is_enabled(DebugTool::Lisp)
            {
                eprintln!("{i}: {:?}", &self.commands[i]);
            }

            match &self.commands[i]
            {
                CommandRaw::Push(register) =>
                {
                    memory.push_stack_register(*register);
                },
                CommandRaw::Pop(register) =>
                {
                    memory.pop_stack_register(*register);
                },
                CommandRaw::Lookup{id, register} =>
                {
                    if let Some(value) = memory.lookup_symbol(*id)
                    {
                        memory.registers[*register as usize] = value;
                    } else
                    {
                        return_error!(Error::UndefinedVariable(memory.get_symbol(*id)), "lookup")
                    }
                },
                CommandRaw::PutRegister{target, source} => memory.set_register(*target, memory.get_register(*source)),
                CommandRaw::PutValue{value, register} => memory.set_register(*register, *value),
                CommandRaw::Jump(destination) =>
                {
                    i = *destination;
                    continue;
                },
                CommandRaw::JumpIfTrue{target, check} =>
                {
                    match memory.get_register(*check).as_bool()
                    {
                        Ok(value) =>
                        {
                            if value
                            {
                                i = *target;
                                continue;
                            }
                        },
                        Err(err) => return_error!(err, "jump if true")
                    }
                },
                CommandRaw::IsPrimitiveProcedure =>
                {
                    let is_primitive = memory.get_register(Register::Operator).tag == ValueTag::PrimitiveProcedure;
                    memory.set_register(Register::Temporary, is_primitive);
                },
                CommandRaw::Cons{target, car, cdr} =>
                {
                    if let Err(err) = memory.cons(*target, *car, *cdr)
                    {
                        return_error!(err, "cons")
                    }
                },
                CommandRaw::CallPrimitiveValue{target} =>
                {
                    let op = memory.get_register(Register::Operator).as_primitive_procedure()
                        .expect("must be checked");

                    let primitive = &self.primitives.get(op).on_apply.as_ref()
                        .expect("primitive must have apply")
                        .1;

                    if let Err(err) = primitive(memory, *target)
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

#[derive(Debug, Clone)]
pub struct Program
{
    memory: LispMemory,
    code: CompiledProgram
}

impl Program
{
    pub fn parse(
        primitives: Rc<Primitives>,
        mut memory: LispMemory,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        let ast = Parser::parse(code)?;

        let ir = InterReprPos::parse(&mut memory, &primitives, ast)?;

        let compiled = {
            let mut state = CompileState::new(&primitives);

            ir.compile(&mut state, Some(Register::Value), Proceed::Jump(Label::Halt))
        };

        let code = compiled.into_program(primitives);

        Ok(Self{memory, code})
    }

    pub fn eval(&self) -> Result<OutputWrapper, ErrorPos>
    {
        let mut memory = self.memory.clone();

        self.code.run(&mut memory)?;

        let value = memory.get_register(Register::Value);
        Ok(OutputWrapper{memory, value})
    }

    pub fn memory_mut(&mut self) -> &mut LispMemory
    {
        &mut self.memory
    }
}
