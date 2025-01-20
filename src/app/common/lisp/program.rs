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
        self.map_err(|value| ErrorPos{position, value})
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
    pub fn new(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_eval: OnEval,
        on_apply: impl Fn(PrimitiveArgs) -> Result<LispValue, Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: Some(on_eval),
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
            ($name:literal, $f:expr) =>
            {
                (PrimitiveName::from($name), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, |mut args|
                {
                    let start = args.next().expect("must have 2 or more args");
                    args.try_fold(start, |acc, x|
                    {
                        Self::call_op(acc, x, |a, b|
                        {
                            Some(LispValue::new_bool($f(a, b)))
                        }, |a, b|
                        {
                            Some(LispValue::new_bool($f(a, b)))
                        })
                    })
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
                (PrimitiveName::from($name), PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let tag = args.next().unwrap().tag;
                    let is_equal = false $(|| tag == $tag)+;

                    Ok(is_equal.into())
                }))
            }
        }

        let make_procedure_tag_check_work = ();
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
            ("if".into(),
                PrimitiveProcedureInfo::new_eval(2..=3, Rc::new(|memory, primitives, args|
                {
                    InterRepr::parse_if(memory, primitives, args)
                }))),
            ("cons".into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let restore = args.memory.with_saved_registers([Register::Temporary, Register::Value]);

                    let car = args.next().unwrap();
                    args.memory.set_register(Register::Temporary, car);

                    let cdr = args.next().unwrap();
                    args.memory.set_register(Register::Value, cdr);

                    args.memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    let value = args.memory.get_register(Register::Value);
                    restore(args.memory);

                    Ok(value)
                })),
            ("car".into(),
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = arg.as_list(args.memory)?.car;

                    Ok(value)
                })),
            ("cdr".into(),
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = arg.as_list(args.memory)?.cdr;

                    Ok(value)
                })),
            ("set-car!".into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let list_id = arg.as_list_id()?;

                    let value = args.next().unwrap();

                    args.memory.set_car(list_id, value);

                    Ok(().into())
                })),
            ("set-cdr!".into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let list_id = arg.as_list_id()?;

                    let value = args.next().unwrap();

                    args.memory.set_cdr(list_id, value);

                    Ok(().into())
                })),
            ("+".into(), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(add, checked_add))),
            ("-".into(), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(sub, checked_sub))),
            ("*".into(), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(mul, checked_mul))),
            ("/".into(), PrimitiveProcedureInfo::new_simple(ArgsCount::Min(2), Effect::Pure, do_op!(div, checked_div))),
            ("remainder".into(), PrimitiveProcedureInfo::new_simple(2, Effect::Pure, do_op_simple!(rem))),
            do_cond!("=", |a, b| a == b),
            do_cond!(">", |a, b| a > b),
            do_cond!("<", |a, b| a < b),
            ("eq?".into(), PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
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
            // is_tag!("procedure?", ValueTag::Address, ValueTag::PrimitiveProcedure),
            is_tag!("number?", ValueTag::Integer, ValueTag::Float),
            ("lambda".into(),
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|memory, primitives, args|
                {
                    Ok(InterRepr::parse_lambda(memory, primitives, args)?)
                }))),
            ("define".into(),
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|memory, primitives, args: AstPos|
                {
                    if args.is_null()
                    {
                        return Err(ErrorPos{position: args.position, value: Error::WrongArgumentsCount{
                            proc: "define".to_owned(),
                            this_invoked: true,
                            expected: ArgsCount::Min(2).to_string(),
                            got: Some(0)
                        }});
                    }

                    let first = args.car();

                    let is_procedure = first.is_list();

                    if is_procedure
                    {
                        /*let position = args.position;

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

                        (Box::new(args), ArgsWrapper::from(2))*/todo!()
                    } else
                    {
                        let name = InterReprPos::parse_symbol(memory, &first)?;

                        let position = args.position;
                        let args = InterReprPos::parse_args(memory, primitives, args.cdr())?;
                        let args_len = args.len();

                        if args_len != 1
                        {
                            return Err(ErrorPos{position, value: Error::WrongArgumentsCount{
                                proc: "define".to_owned(),
                                this_invoked: true,
                                expected: "2".to_owned(),
                                got: Some(args_len + 1)
                            }});
                        }

                        Ok(InterRepr::Define{
                            name,
                            body: Box::new(args.into_iter().next().unwrap())
                        })
                    }
                }))),
            ("let".into(),
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|memory, primitives, args|
                {
                    /*let bindings = args.car();
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
                    })*/todo!()
                }))),
            ("make-vector".into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let len = args.next().unwrap().as_integer()? as usize;
                    let fill = args.next().unwrap();

                    let vec = LispVectorRef{
                        tag: fill.tag,
                        values: &vec![fill.value; len]
                    };

                    todo!()
                    // memory.allocate_vector(vec)
                })),
            ("vector-set!".into(),
                PrimitiveProcedureInfo::new_simple(3, Effect::Impure, |mut args|
                {
                    let vec = args.next().unwrap();
                    let index = args.next().unwrap();
                    let value = args.next().unwrap();

                    let vec = vec.as_vector_mut(args.memory)?;

                    let index = index.as_integer()?;

                    if vec.tag != value.tag
                    {
                        return Err(
                            Error::VectorWrongType{expected: vec.tag, got: value.tag}
                        );
                    }

                    *vec.values.get_mut(index as usize)
                        .ok_or(Error::IndexOutOfRange(index))? = value.value;

                    Ok(().into())
                })),
            ("vector-ref".into(),
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let vec = args.next().unwrap();
                    let index = args.next().unwrap();

                    let vec = vec.as_vector_ref(args.memory)?;
                    let index = index.as_integer()?;

                    let value = vec.try_get(index as usize).ok_or(Error::IndexOutOfRange(index))?;

                    Ok(value)
                })),
            ("display".into(),
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, |mut args|
                {
                    let arg = args.next().unwrap();

                    print!("{}", arg.to_string(args.memory));
                    io::stdout().flush().unwrap();

                    Ok(().into())
                })),
            ("newline".into(),
                PrimitiveProcedureInfo::new_simple(0, Effect::Impure, |_args|
                {
                    println!();

                    Ok(().into())
                })),
            ("random-integer".into(),
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, |mut args|
                {
                    let limit = args.next().unwrap().as_integer()?;

                    Ok(fastrand::i32(0..limit).into())
                })),
            ("exact->inexact".into(),
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
            ("inexact->exact".into(),
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
}

#[derive(Debug, Clone)]
enum Command
{
    Push(Register),
    Pop(Register),
    PutValue{value: LispValue, register: Register},
    Move{target: Register, source: Register},
    Lookup{id: SymbolId, register: Register},
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
    Error(ErrorPos),
    Label(Label)
}

impl Command
{
    pub fn modifies_registers(&self) -> Vec<Register>
    {
        match self
        {
            Self::PutValue{register, ..}
            | Self::Lookup{register, ..}
            | Self::PutLabel{target: register, ..}
            | Self::Move{target: register, ..}
            | Self::Pop(register)
            | Self::Cons{target: register, ..}
            | Self::Car{target: register, ..}
            | Self::Cdr{target: register, ..}
            | Self::CallPrimitiveValue{target: register, ..} => vec![*register],
            Self::Define{..}
            | Self::CreateChildEnvironment => vec![Register::Environment, Register::Value, Register::Temporary],
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
            Self::Lookup{id, register} => CommandRaw::Lookup{id, register},
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
            .chain(self.commands.into_iter())
            .chain(save_registers.rev().map(|(register, _)| -> CommandPos
            {
                Command::Pop(register).into()
            }))
            .chain(other.commands)
            .collect();

        Self{
            modifies: self.modifies.union(other.modifies),
            requires: self.requires.union(other.requires),
            commands
        }
    }

    pub fn with_proceed(self, proceed: Proceed) -> Self
    {
        self.combine_preserving(proceed.into_compiled(), RegisterStates::one(Register::Return))
    }

    pub fn into_program(mut self, state: CompileState, primitives: Rc<Primitives>) -> CompiledProgram
    {
        self.commands.push(Command::Jump(Label::Halt).into());

        state.lambdas.into_iter().for_each(|lambda|
        {
            self.commands.extend(lambda.commands);
        });

        self.commands.push(Command::Label(Label::Halt).into());

        if DebugConfig::is_enabled(DebugTool::Lisp)
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

                eprint!("{value:?}");

                if let Some(position) = position
                {
                    eprint!(" ({position})");
                }

                eprintln!();
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
pub enum LambdaParams
{
    Variadic(SymbolId),
    Normal(Vec<SymbolId>)
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
            Ast::Value(_) =>
            {
                Ok(Self::Variadic(InterReprPos::parse_symbol(memory, &ast)?))
            },
            Ast::List{..} => Ok(Self::Normal(Self::parse_list(memory, ast)?)),
            Ast::EmptyList => Err(ErrorPos{position: ast.position, value: Error::ExpectedParam})
        }
    }

    pub fn parse_list(memory: &mut LispMemory, ast: AstPos) -> Result<Vec<SymbolId>, ErrorPos>
    {
        match ast.value
        {
            Ast::List{car, cdr} =>
            {
                let tail = Self::parse_list(memory, *cdr)?;
                let symbol = InterReprPos::parse_symbol(memory, &car)?;

                Ok(iter::once(symbol).chain(tail).collect())
            },
            Ast::EmptyList => Ok(Vec::new()),
            Ast::Value(_) => unreachable!("malformed ast")
        }
    }

    fn compile(self) -> CompiledPart
    {
        match self
        {
            Self::Variadic(id) => Command::Define{id, register: Register::Argument}.into(),
            Self::Normal(params) =>
            {
                let commands = params.into_iter().flat_map(|param|
                {
                    [
                        Command::Car{target: Register::Temporary, source: Register::Argument},
                        Command::Define{id: param, register: Register::Temporary},
                        Command::Cdr{target: Register::Argument, source: Register::Argument}
                    ]
                }).map(|x| CommandPos::from(x)).collect::<Vec<_>>();

                CompiledPart::from_commands(commands)
            }
        }
    }
}

#[derive(Debug)]
pub enum InterRepr
{
    Apply{op: Box<InterReprPos>, args: Vec<InterReprPos>},
    Sequence(Vec<InterReprPos>),
    If{check: Box<InterReprPos>, then: Box<InterReprPos>, else_body: Box<InterReprPos>},
    Define{name: SymbolId, body: Box<InterReprPos>},
    Lambda{params: LambdaParams, body: Box<InterReprPos>},
    Lookup(SymbolId),
    Value(LispValue)
}

impl InterReprPos
{
    pub fn parse_symbol(memory: &mut LispMemory, ast: &AstPos) -> Result<SymbolId, ErrorPos>
    {
        let position = ast.position;
        Self::parse_primitive_value(memory, ast).and_then(|x| x.as_symbol_id().with_position(position))
    }

    pub fn parse_primitive_value(memory: &mut LispMemory, ast: &AstPos) -> Result<LispValue, ErrorPos>
    {
        if let Ast::Value(ref x) = ast.value
        {
            InterReprPos::parse_primitive_text(memory, x).with_position(ast.position)
        } else
        {
            Err(ErrorPos{position: ast.position, value: Error::ExpectedSymbol})
        }
    }

    pub fn parse_primitive_text(memory: &mut LispMemory, text: &str) -> Result<LispValue, Error>
    {
        Ok(memory.new_primitive_value(Ast::parse_primitive(&text)?))
    }

    pub fn parse(
        memory: &mut LispMemory,
        primitives: &Primitives,
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
        let args_error = |count|
        {
            return Err(Error::WrongArgumentsCount{
                proc: QUOTE_PRIMITIVE.to_owned(),
                this_invoked: false,
                expected: "1".to_owned(),
                got: count
            }).with_position(ast.position);
        };

        let value = if let Ast::List{car, cdr} = ast.value
        {
            if cdr.is_null()
            {
                *car
            } else
            {
                return args_error(None);
            }
        } else
        {
            return args_error(Some(0));
        };

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
                let value: Result<_, ErrorPos> = Self::parse_primitive_text(memory, &x).with_position(ast.position);

                value?
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
            InterRepr::Lookup(id) =>
            {
                if let Some(register) = target
                {
                    CompiledPart::from_commands(vec![Command::Lookup{id, register}.with_position(self.position)])
                        .with_requires(RegisterStates::one(Register::Environment))
                } else
                {
                    CompiledPart::new()
                }.with_proceed(proceed)
            },
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
                }).reduce(CompiledPart::combine).unwrap_or_else(CompiledPart::new)
            },
            InterRepr::If{check, then, else_body} =>
            {
                let else_branch = Label::ElseBranch(state.label_id());

                let check_pos = check.position;
                let check_value = check.compile(state, Some(Register::Value), Proceed::Next);

                let type_check = if state.type_checks
                {
                    let error_branch = Label::ErrorBranch(state.label_id());
                    let post_branch = Label::AfterError(state.label_id());

                    CompiledPart::from_commands(vec![
                        Command::IsTag{check: Register::Value, tag: ValueTag::List}.into(),
                        Command::JumpIfTrue{target: post_branch, check: Register::Temporary}.into(),
                        Command::Label(error_branch).into(),
                        Command::Error(Error::WrongConditionalType(String::new()).with_position(check_pos)).into(),
                        Command::Label(post_branch).into()
                    ])
                } else
                {
                    CompiledPart::new()
                };

                let check = check_value.combine(type_check)
                    .combine(Command::JumpIfFalse{target: else_branch, check: Register::Value});

                let after_if_label = Label::AfterIf(state.label_id());

                let then_proceed = match proceed
                {
                    Proceed::Next => Proceed::Jump(after_if_label),
                    x => x
                };

                let then_part = then.compile(state, target, then_proceed);
                let else_part = CompiledPart::from(Command::Label(else_branch))
                    .combine(else_body.compile(state, target, proceed));

                let if_body = check.combine_preserving(
                    then_part.combine(else_part),
                    RegisterStates::one(Register::Environment)
                );

                if let Proceed::Next = proceed
                {
                    if_body.combine(Command::Label(after_if_label))
                } else
                {
                    if_body
                }
            },
            InterRepr::Lambda{params, body} =>
            {
                let target = if let Some(target) = target
                {
                    target
                } else
                {
                    return CompiledPart::new();
                };

                let params_define = params.compile();
                let body = body.compile(state, Some(Register::Value), Proceed::Return);
                let label = state.add_lambda(params_define.combine(body));

                let label_part: CompiledPart = Command::PutLabel{target, label}.into();

                let env_part = CompiledPart::from_commands(vec![
                    Command::CreateChildEnvironment.with_position(self.position)
                ]).with_requires(RegisterStates::one(Register::Environment));

                let cons_part = CompiledPart::from_commands(vec![
                    Command::Cons{target, car: Register::Environment, cdr: target}.with_position(self.position)
                ]).with_requires(RegisterStates::one(target));

                let lambda = label_part.combine(env_part.combine_preserving(cons_part, RegisterStates::one(target)));

                lambda.with_proceed(proceed)
            },
            InterRepr::Define{name, body} =>
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
                body.combine_preserving(CompiledPart::from_commands(commands), RegisterStates::one(Register::Environment))
                    .with_proceed(proceed)
                    .with_requires(RegisterStates::one(Register::Environment))
            },
            InterRepr::Apply{op, args} =>
            {
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

                    acc.combine_preserving(body, RegisterStates::one(Register::Argument))
                });

                let operator_setup = op.compile(state, Some(Register::Operator), Proceed::Next);

                let primitive_return: CompiledPart = match proceed
                {
                    Proceed::Jump(label) => Command::Jump(label).into(),
                    Proceed::Next => CompiledPart::new(),
                    Proceed::Return => Command::JumpRegister(Register::Return).into()
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

                let compound_part_basic = compound_check
                    .combine(compound_call_part)
                    .combine(compound_error)
                    .with_requires(RegisterStates::one(Register::Environment));

                let after_procedure = Label::AfterProcedure(state.label_id());

                let compound_part = if target.is_none() || target == Some(Register::Value)
                {
                    let prepare_return = match proceed
                    {
                        Proceed::Jump(label) => Command::PutLabel{target: Register::Return, label}.into(),
                        Proceed::Next => Command::PutLabel{target: Register::Return, label: after_procedure}.into(),
                        Proceed::Return => CompiledPart::new()
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
                        Proceed::Next => Command::Jump(after_procedure).into(),
                        _ => proceed.into_compiled()
                    };

                    prepare_return.combine(compound_part_basic).combine(CompiledPart::from_commands(vec![
                        Command::Label(procedure_return).into(),
                        Command::Move{target: target.expect("checked in branch"), source: Register::Value}.into()
                    ])).combine(proceed)
                };

                let primitive_part = CompiledPart::from_commands(vec![
                    Command::Label(primitive_branch).into(),
                    Command::CallPrimitiveValue{target: target.unwrap_or(Register::Temporary)}.with_position(self.position)
                ]).combine(primitive_return);

                let call_part = check_part.combine(compound_part).combine(primitive_part);

                let after_procedure = if proceed == Proceed::Next
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
                    RegisterStates::one(Register::Operator).set(Register::Environment)
                );

                operator_setup.combine(after_operator)
            }
        }
    }
}

impl InterRepr
{
    pub fn parse_if(
        memory: &mut LispMemory,
        primitives: &Primitives,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let position = ast.position;
        let args = InterReprPos::parse_args(memory, primitives, ast)?;

        if !(2..=3).contains(&args.len())
        {
            return Err(Error::WrongArgumentsCount{
                proc: "if".to_owned(),
                this_invoked: false,
                expected: ArgsCount::from(2..=3).to_string(),
                got: Some(args.len())
            }).with_position(position);
        }

        let mut args = args.into_iter();

        let check = Box::new(args.next().unwrap());
        let then_body = Box::new(args.next().unwrap());

        let else_body = Box::new(args.next().unwrap_or_else(||
        {
            Self::Value(LispValue::new_empty_list()).with_position(then_body.position)
        }));

        Ok(Self::If{check, then: then_body, else_body})
    }

    pub fn parse_lambda(
        memory: &mut LispMemory,
        primitives: &Primitives,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let args_error = |count|
        {
            Err(Error::WrongArgumentsCount{
                proc: "lambda".to_owned(),
                this_invoked: false,
                expected: ArgsCount::Min(2).to_string(),
                got: Some(count)
            }).with_position(ast.position)
        };

        let (params, body) = if let Ast::List{car, cdr} = ast.value
        {
            let bodies_position = cdr.position;

            let bodies = InterReprPos::parse_args(memory, primitives, *cdr)?;
            if bodies.is_empty()
            {
                return args_error(1);
            }

            let body = InterRepr::Sequence(bodies).with_position(bodies_position);

            (*car, body)
        } else
        {
            return args_error(0);
        };

        let params = LambdaParams::parse(memory, params)?;

        Ok(Self::Lambda{params, body: Box::new(body)})
    }
}

#[derive(Debug)]
struct CompileState
{
    type_checks: bool,
    lambdas: Vec<CompiledPart>,
    label_id: u32
}

impl CompileState
{
    pub fn new(type_checks: bool) -> Self
    {
        Self{
            type_checks,
            lambdas: Vec::new(),
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

        self.lambdas.push(
            CompiledPart::from(Command::Label(label))
                .combine(lambda)
                .combine(Command::JumpRegister(Register::Return))
        );

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
    Lookup{id: SymbolId, register: Register},
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
                        value: $error
                    })
                }
            }

            if DebugConfig::is_enabled(DebugTool::Lisp)
            {
                eprintln!("[RUNNING] {i}: {:?}", &self.commands[i]);
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
                        memory.set_register(*register, value);
                    } else
                    {
                        return_error!(Error::UndefinedVariable(memory.get_symbol(*id)), "lookup")
                    }
                },
                CommandRaw::Define{id, register} =>
                {
                    let value = memory.get_register(*register);
                    if let Err(err) = memory.define_symbol(*id, value)
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
                    let value = memory.get_register(*source).as_list(memory)
                        .expect("must be a list")
                        .car;

                    memory.set_register(*target, value);
                },
                CommandRaw::Cdr{target, source} =>
                {
                    let value = memory.get_register(*source).as_list(memory)
                        .expect("must be a list")
                        .cdr;

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
                        _ => ()
                    }

                    return Err(err);
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

        let code = {
            let mut state = CompileState::new(true);

            let compiled = ir.compile(&mut state, Some(Register::Value), Proceed::Jump(Label::Halt));

            compiled.into_program(state, primitives)
        };

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
