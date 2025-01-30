use std::{
    vec,
    iter,
    rc::Rc,
    io::{self, Write},
    fmt::{self, Debug, Display},
    collections::HashMap,
    ops::{RangeInclusive, Add, Sub, Mul, Div, Rem, Index, IndexMut}
};

use strum::{EnumCount, FromRepr};

use crate::debug_config::*;

pub use super::{
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
    pub memory: &'a mut LispMemory
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

    pub fn new_with_target(
        args_count: impl Into<ArgsCount>,
        effect: Effect,
        on_apply: impl Fn(PrimitiveArgs, Register) -> Result<(), Error> + 'static
    ) -> Self
    {
        Self{
            args_count: args_count.into(),
            on_eval: None,
            on_apply: Some((effect, Rc::new(move |memory, target|
            {
                on_apply(PrimitiveArgs{memory}, target)
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
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(1), Rc::new(|memory, args|
                {
                    Ok(InterRepr::Sequence(InterReprPos::parse_args(memory, args)?))
                }))),
            (QUOTE_PRIMITIVE,
                PrimitiveProcedureInfo::new_eval(1, Rc::new(|_memory, args|
                {
                    Ok(InterRepr::Quoted(args.car()))
                }))),
            ("if",
                PrimitiveProcedureInfo::new_eval(2..=3, Rc::new(|memory, args|
                {
                    InterRepr::parse_if(memory, args)
                }))),
            ("cons",
                PrimitiveProcedureInfo::new_simple(2, Effect::Pure, |mut args|
                {
                    let restore = args.memory.with_saved_registers([Register::Value]);

                    let car = args.next().unwrap();
                    args.memory.set_register(Register::Temporary, car);

                    let cdr = args.next().unwrap();
                    args.memory.set_register(Register::Value, cdr);

                    args.memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    let value = args.memory.get_register(Register::Value);
                    restore(args.memory);

                    Ok(value)
                })),
            ("car",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = arg.as_list(args.memory)?.car;

                    Ok(value)
                })),
            ("cdr",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, |mut args|
                {
                    let arg = args.next().unwrap();
                    let value = arg.as_list(args.memory)?.cdr;

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
                    value.as_list(args.memory).map(|x|
                    {
                        x.cdr.tag == ValueTag::Address
                    }).unwrap_or(false)
                };

                let is_equal = value.tag == ValueTag::PrimitiveProcedure || is_compound();

                Ok(is_equal.into())
            })),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(2), Rc::new(|memory, args|
                {
                    Ok(InterRepr::parse_lambda(memory, "<lambda>".to_owned(), args)?)
                }))),
            ("define",
                PrimitiveProcedureInfo::new_eval(ArgsCount::Min(2), Rc::new(|memory, args: AstPos|
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
                        let lambda = InterRepr::parse_lambda(memory, lambda_name, lambdas_body)?
                            .with_position(position);

                        (name, lambda)
                    } else
                    {
                        let name = InterReprPos::parse_symbol(memory, &first)?;
                        let args = InterReprPos::parse_args(memory, args.cdr())?;

                        (name, args.into_iter().next().unwrap())
                    };

                    Ok(InterRepr::Define{
                        name,
                        body: Box::new(value)
                    })
                }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Rc::new(|memory, args|
                {
                    Ok(InterRepr::parse_let(memory, args)?)
                }))),
            ("make-vector",
                PrimitiveProcedureInfo::new_with_target(2, Effect::Pure, |mut args, target|
                {
                    let len = args.next().unwrap().as_integer()? as usize;
                    let fill = args.next().unwrap();

                    args.memory.make_vector(target, vec![fill; len])
                })),
            ("vector-set!",
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
            ("random-integer",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, |mut args|
                {
                    let limit = args.next().unwrap().as_integer()?;

                    Ok(fastrand::i32(0..limit).into())
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
    CallPrimitiveValueUnchecked{target: Register},
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
            .chain(self.commands.into_iter())
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

    pub fn into_program(mut self, state: CompileState) -> CompiledProgram
    {
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
            Ast::Value(_) =>
            {
                Ok(Self::Variadic(InterReprPos::parse_symbol(memory, &ast)?))
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
            Ast::Value(_) => unreachable!("malformed ast")
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
            Self::Variadic(id) => Command::Define{id, register: Register::Argument}.into(),
            Self::Normal(params) =>
            {
                let amount = params.len();
                let commands = params.into_iter().enumerate().flat_map(|(index, WithPosition{position, value: param})|
                {
                    let is_last = amount == (index + 1);

                    let define_one = |include_tail|
                    {
                        let mut commands = vec![
                            Command::Car{target: Register::Temporary, source: Register::Argument},
                            Command::Define{id: param, register: Register::Temporary}
                        ];

                        if include_tail
                        {
                            commands.push(Command::Cdr{target: Register::Argument, source: Register::Argument});
                        }

                        commands
                    };

                    let include_tail = state.type_checks || !is_last;

                    let commands = define_one(include_tail);

                    if state.type_checks
                    {
                        let after_little_error = Label::AfterError(state.label_id());

                        let mut commands: Vec<_> = [
                            Command::IsTag{check: Register::Argument, tag: ValueTag::List},
                            Command::JumpIfTrue{target: after_little_error, check: Register::Temporary},
                            Command::Error(ErrorPos{position, value: Error::WrongArgumentsCount{
                                proc: name.clone(),
                                expected: amount.to_string(),
                                got: index
                            }}),
                            Command::Label(after_little_error)
                        ].into_iter().chain(commands).collect();

                        if is_last
                        {
                            let after_error = Label::AfterError(state.label_id());

                            commands.extend([
                                Command::IsTag{check: Register::Argument, tag: ValueTag::EmptyList},
                                Command::JumpIfTrue{target: after_error, check: Register::Temporary},
                                Command::Error(ErrorPos{position, value: Error::WrongArgumentsCount{
                                    proc: name.clone(),
                                    expected: amount.to_string(),
                                    got: amount
                                }}),
                                Command::Label(after_error)
                            ]);
                        }

                        commands
                    } else
                    {
                        commands
                    }
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
    Lambda{name: String, params: LambdaParams, body: Box<InterReprPos>},
    Quoted(AstPos),
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
                let op = Self::parse(memory, *car)?;

                if let InterRepr::Value(value) = op.value
                {
                    if let Ok(id) = value.as_primitive_procedure()
                    {
                        let primitive = memory.primitives.get(id).clone();
                        if let Some(on_eval) = &primitive.on_eval
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

                            return on_eval(memory, args).map(|x| x.with_position(ast.position));
                        }
                    }
                }

                let args = Self::parse_args(memory, *cdr)?;

                Ok(InterRepr::Apply{op: Box::new(op), args}.with_position(ast.position))
            }
        }
    }

    pub fn parse_args(
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Vec<Self>, ErrorPos>
    {
        match ast.value
        {
            Ast::Value(_) => unreachable!("malformed ast"),
            Ast::EmptyList => Ok(Vec::new()),
            Ast::List{car, cdr} =>
            {
                let tail = Self::parse_args(memory, *cdr)?;

                Ok(iter::once(Self::parse(memory, *car)?).chain(tail).collect())
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

    fn is_known_compound(&self) -> bool
    {
        if let InterRepr::Lambda{..} = self.value
        {
            true
        } else
        {
            false
        }
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

                let lambda = label_part.combine(cons_part);

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
                body.combine_preserving(
                    CompiledPart::from_commands(commands).with_requires(RegisterStates::one(Register::Environment)),
                    RegisterStates::one(Register::Environment)
                ).with_proceed(proceed)
            },
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
                let is_known_compound = op.is_known_compound();

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
}

impl InterRepr
{
    pub fn parse_if(
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let args = InterReprPos::parse_args(memory, ast)?;

        let mut args = args.into_iter();

        let check = Box::new(args.next().unwrap());
        let then_body = Box::new(args.next().unwrap());

        let else_body = Box::new(args.next().unwrap_or_else(||
        {
            Self::Value(LispValue::new_empty_list()).with_position(then_body.position)
        }));

        Ok(Self::If{check, then: then_body, else_body})
    }

    pub fn parse_let(
        memory: &mut LispMemory,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let position = ast.position;

        let params_ast = ast.car();
        let body = InterReprPos::parse_args(memory, ast.cdr())?
            .into_iter().next().unwrap();

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

            args.push(InterReprPos::parse(memory, arg)?);

            Ok(())
        })?;

        let params = LambdaParams::Normal(params);

        let lambda = Self::Lambda{name: "<lambda>".to_owned(), params, body: Box::new(body)}
            .with_position(position);

        Ok(Self::Apply{op: Box::new(lambda), args})
    }

    pub fn parse_lambda(
        memory: &mut LispMemory,
        name: String,
        ast: AstPos
    ) -> Result<Self, ErrorPos>
    {
        let params = ast.car();

        let cdr = ast.cdr();

        let bodies_position = cdr.position;

        let bodies = InterReprPos::parse_args(memory, cdr)?;
        let body = InterRepr::Sequence(bodies).with_position(bodies_position);

        let params = LambdaParams::parse(memory, params)?;

        Ok(Self::Lambda{name, params, body: Box::new(body)})
    }
}

#[derive(Debug)]
struct CompileState<'a>
{
    pub memory: &'a mut LispMemory,
    type_checks: bool,
    lambdas: Vec<CompiledPart>,
    label_id: u32
}

impl<'a> CompileState<'a>
{
    pub fn new(memory: &'a mut LispMemory, type_checks: bool) -> Self
    {
        Self{
            memory,
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
    CallPrimitiveValue{target: Register},
    CallPrimitiveValueUnchecked{target: Register}
}

struct DebugRaw(String);

impl Debug for DebugRaw
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.0)
    }
}

struct CommandRawDisplay<'a, 'b>
{
    memory: &'a mut LispMemory,
    value: &'b CommandRaw
}

impl Debug for CommandRawDisplay<'_, '_>
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
            CommandRaw::Lookup{id, register} =>
            {
                f.debug_struct("Lookup")
                    .field("id", &DebugRaw(LispValue::new_symbol_id(*id).to_string(self.memory)))
                    .field("register", register)
                    .finish()
            },
            CommandRaw::Define{id, register} =>
            {
                f.debug_struct("Define")
                    .field("id", &DebugRaw(LispValue::new_symbol_id(*id).to_string(self.memory)))
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

            let command = &self.commands[i];

            if DebugConfig::is_enabled(DebugTool::Lisp)
            {
                eprintln!("[RUNNING] {i}: {:?}", CommandRawDisplay{memory, value: command});
            }

            match command
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
                        Error::WrongArgumentsCount{got, ..} =>
                        {
                            let mut leftover = 0;

                            while let Ok(lst) = memory.get_register(Register::Argument).as_list(memory)
                            {
                                memory.set_register(Register::Argument, lst.cdr);

                                leftover += 1;
                            }

                            *got = *got + leftover;
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
        type_checks: bool,
        mut memory: LispMemory,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        debug_assert!(memory.iter_values().all(|x| x.tag != ValueTag::Address));

        let ast = Parser::parse(code)?;

        let ir = InterReprPos::parse(&mut memory, ast)?;

        let code = {
            let type_checks = type_checks && DebugConfig::is_disabled(DebugTool::LispDisableChecks);
            let mut state = CompileState::new(&mut memory, type_checks);

            let compiled = ir.compile(&mut state, Some(Register::Value), Proceed::Jump(Label::Halt));

            compiled.into_program(state)
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
