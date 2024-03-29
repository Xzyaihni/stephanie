use std::{
    vec,
    iter,
    ops::{Add, Sub, Mul, Div, Rem}
};

pub use super::{Error, Environment, LispValue, LispMemory, ValueTag};
use parser::{Parser, Ast, PrimitiveType};

mod parser;


#[derive(Debug, Clone, Copy)]
pub enum PrimitiveProcedure
{
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    IsEqual,
    IsGreater,
    IsLess,
    Lambda,
    Define,
    Quote,
    If,
    Cons,
    Car,
    Cdr
}

impl PrimitiveProcedure
{
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
        mut args: ArgValues,
        op_integer: FI,
        op_float: FF
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> LispValue,
        FF: Fn(f32, f32) -> LispValue
    {
        let first = args.pop()?;
        let second = args.pop()?;

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        let is_true = output.as_bool()?;

        if !is_true || args.is_empty()
        {
            Ok(output)
        } else
        {
            args.push(second);

            Self::do_cond(args, op_integer, op_float)
        }
    }

    fn do_op<FI, FF>(
        mut args: ArgValues,
        op_integer: FI,
        op_float: FF
    ) -> Result<LispValue, Error>
    where
        FI: Fn(i32, i32) -> LispValue,
        FF: Fn(f32, f32) -> LispValue
    {
        let first = args.pop()?;
        let second = args.pop()?;

        let output = Self::call_op(first, second, &op_integer, &op_float)?;

        if args.is_empty()
        {
            Ok(output)
        } else
        {
            args.push(output);

            Self::do_op(args, op_integer, op_float)
        }
    }

    fn apply(
        self,
        lambdas: &Lambdas, 
        memory: &mut LispMemory,
        env: &mut Environment,
        args: &Expression
    ) -> Result<LispValue, Error>
    {
        macro_rules! do_cond
        {
            ($f:expr) =>
            {
                {
                    let args = Expression::apply_args(lambdas, memory, env, args)?;

                    Self::do_cond(args, $f, $f)
                }
            }
        }

        macro_rules! do_op
        {
            ($op:ident) =>
            {
                {
                    let args = Expression::apply_args(lambdas, memory, env, args)?;

                    Self::do_op(args, |a, b|
                    {
                        LispValue::new_integer(a.$op(b))
                    }, |a, b|
                    {
                        LispValue::new_float(a.$op(b))
                    })
                }
            }
        }

        match self
        {
            Self::Add => do_op!(add),
            Self::Sub => do_op!(sub),
            Self::Mul => do_op!(mul),
            Self::Div => do_op!(div),
            Self::Rem => do_op!(rem),
            Self::IsEqual => do_cond!(|a, b| LispValue::new_bool(a == b)),
            Self::IsGreater => do_cond!(|a, b| LispValue::new_bool(a > b)),
            Self::IsLess => do_cond!(|a, b| LispValue::new_bool(a < b)),
            Self::Define =>
            {
                let first = args.car();
                let second = args.cdr().car();

                let key = first.as_value()?;
                let value = second.apply(lambdas, memory, env)?;

                env.define(key, value);

                return Ok(LispValue::new_integer(0));
            },
            Self::If =>
            {
                let predicate = args.car().apply(lambdas, memory, env)?;
                let on_true = args.cdr().car();
                let on_false = args.cdr().cdr().car();

                if predicate.is_true()
                {
                    on_true.apply(lambdas, memory, env)
                } else
                {
                    on_false.apply(lambdas, memory, env)
                }
            },
            Self::Cons =>
            {
                let mut args = Expression::apply_args(lambdas, memory, env, args)?;

                let car = args.pop()?;
                let cdr = args.pop()?;

                Ok(memory.cons(car, cdr))
            },
            Self::Car =>
            {
                let value = args.car().apply(lambdas, memory, env)?
                    .as_list(memory)?;

                Ok(value.car)
            },
            Self::Cdr =>
            {
                let value = args.car().apply(lambdas, memory, env)?
                    .as_list(memory)?;

                Ok(value.cdr)
            },
            Self::Quote =>
            {
                Ok(memory.allocate_expression(args))
            },
            Self::Lambda => unreachable!()
        }
    }
}

#[derive(Debug, Clone)]
pub enum CompoundProcedure
{
    Identifier(String),
    Lambda(usize)
}

#[derive(Debug, Clone)]
pub enum Procedure
{
    Compound(CompoundProcedure),
    Primitive(PrimitiveProcedure)
}

impl From<String> for Procedure
{
    fn from(s: String) -> Self
    {
        match s.as_ref()
        {
            "+" => Self::Primitive(PrimitiveProcedure::Add),
            "-" => Self::Primitive(PrimitiveProcedure::Sub),
            "*" => Self::Primitive(PrimitiveProcedure::Mul),
            "/" => Self::Primitive(PrimitiveProcedure::Div),
            "=" => Self::Primitive(PrimitiveProcedure::IsEqual),
            ">" => Self::Primitive(PrimitiveProcedure::IsGreater),
            "<" => Self::Primitive(PrimitiveProcedure::IsLess),
            "remainder" => Self::Primitive(PrimitiveProcedure::Rem),
            "lambda" => Self::Primitive(PrimitiveProcedure::Lambda),
            "define" => Self::Primitive(PrimitiveProcedure::Define),
            "if" => Self::Primitive(PrimitiveProcedure::If),
            "quote" => Self::Primitive(PrimitiveProcedure::Quote),
            "cons" => Self::Primitive(PrimitiveProcedure::Cons),
            "car" => Self::Primitive(PrimitiveProcedure::Car),
            "cdr" => Self::Primitive(PrimitiveProcedure::Cdr),
            _ => Self::Compound(CompoundProcedure::Identifier(s))
        }
    }
}

// i dont wanna store the body over and over in the virtual memory
// but this seems silly, so i dunno >~<
#[derive(Debug, Clone)]
pub struct StoredLambda
{
    params: ArgValues<String>,
    body: Expression
}

impl StoredLambda
{
    pub fn new(params: Expression, body: Expression) -> Result<Self, Error>
    {
        let params = params.map_list(|arg|
        {
            arg.as_value()
        })?;

        Ok(Self{params, body})
    }

    pub fn apply(
        &self,
        lambdas: &Lambdas,
        memory: &mut LispMemory,
        env: &Environment,
        args: ArgValues
    ) -> Result<LispValue, Error>
    {
        if self.params.len() != args.len()
        {
            return Err(Error::WrongArgumentsCount);
        }

        let mut new_env = Environment::child(env);
        self.params.iter().zip(args.into_iter()).for_each(|(key, value)|
        {
            new_env.define(key, value);
        });

        self.body.apply(lambdas, memory, &mut new_env)
    }
}

#[derive(Debug, Clone)]
pub struct Lambdas
{
    lambdas: Vec<StoredLambda>
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

        self.lambdas.push(lambda);

        id
    }

    pub fn get(&self, index: usize) -> &StoredLambda
    {
        &self.lambdas[index]
    }
}

#[derive(Debug, Clone)]
pub struct Program
{
    lambdas: Lambdas,
    expression: Expression
}

impl Program
{
    pub fn parse(code: &str) -> Result<Self, Error>
    {
        let ast = Parser::parse(code)?;

        let mut lambdas = Lambdas::new();
        let expression = Expression::eval_sequence(&mut lambdas, ast)?;

        Ok(Self{lambdas, expression})
    }

    pub fn apply(
        &self,
        memory: &mut LispMemory,
        env: &mut Environment
    ) -> Result<LispValue, Error>
    {
        self.expression.apply(&self.lambdas, memory, env)
    }
}

#[derive(Debug, Clone)]
pub struct ArgValues<T=LispValue>(Vec<T>);

impl<T> ArgValues<T>
{
    pub fn new() -> Self
    {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool
    {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize
    {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item=&T>
    {
        self.0.iter().rev()
    }

    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item=&mut T>
    {
        self.0.iter_mut().rev()
    }

    pub fn pop(&mut self) -> Result<T, Error>
    {
        let top = self.0.pop();

        top.ok_or(Error::ExpectedArg)
    }

    pub fn push(&mut self, value: T)
    {
        self.0.push(value);
    }
}

impl<T> IntoIterator for ArgValues<T>
{
    type Item = T;
    type IntoIter = iter::Rev<vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter
    {
        self.0.into_iter().rev()
    }
}

#[derive(Debug, Clone)]
pub enum Expression
{
    Value(String),
    Float(f32),
    Integer(i32),
    EmptyList,
    Lambda(usize),
    List{car: Box<Self>, cdr: Box<Self>},
    Application{op: Procedure, args: Box<Self>},
    Sequence{first: Box<Self>, after: Box<Self>}
}

impl Expression
{
    fn car(&self) -> &Self
    {
        match self
        {
            Self::List{car, ..} => car,
            x => panic!("car must be called on a list, called on {x:?}")
        }
    }

    fn cdr(&self) -> &Self
    {
        match self
        {
            Self::List{cdr, ..} => cdr,
            x => panic!("cdr must be called on a list, called on {x:?}")
        }
    }

    fn cons(car: Self, cdr: Self) -> Self
    {
        Self::List{car: Box::new(car), cdr: Box::new(cdr)}
    }

    fn is_null(&self) -> bool
    {
        match self
        {
            Self::EmptyList => true,
            _ => false
        }
    }

    fn as_value(&self) -> Result<String, Error>
    {
        match self
        {
            Self::Value(x) => Ok(x.clone()),
            _ => Err(Error::ExpectedOp)
        }
    }

    pub fn apply(
        &self,
        lambdas: &Lambdas, 
        memory: &mut LispMemory,
        env: &mut Environment
    ) -> Result<LispValue, Error>
    {
        dbg!(self);

        match self
        {
            Self::Integer(x) => Ok(LispValue::new_integer(*x)),
            Self::Float(x) => Ok(LispValue::new_float(*x)),
            Self::Value(s) =>
            {
                env.lookup(s)
            },
            Self::Lambda(id) =>
            {
                Ok(LispValue::new_procedure(*id))
            },
            Self::Application{op, args} =>
            {
                match op
                {
                    Procedure::Compound(p) => Self::apply_compound(lambdas, memory, env, p, args),
                    Procedure::Primitive(p) => p.apply(lambdas, memory, env, args)
                }
            },
            Self::Sequence{first, after} =>
            {
                first.apply(lambdas, memory, env)?;

                after.apply(lambdas, memory, env)
            },
            _ => Err(Error::ApplyNonApplication)
        }
    }

    fn map_list<T, F>(&self, mut f: F) -> Result<ArgValues<T>, Error>
    where
        F: FnMut(&Self) -> Result<T, Error>
    {
        // could be done iteratively but im lazy
        if self.is_null()
        {
            Ok(ArgValues::new())
        } else
        {
            let car = f(self.car())?;
            let cdr = self.cdr();

            let mut args = cdr.map_list(f)?;

            args.push(car);

            Ok(args)
        }
    }

    fn apply_args(
        lambdas: &Lambdas, 
        memory: &mut LispMemory,
        env: &mut Environment,
        args: &Self
    ) -> Result<ArgValues, Error>
    {
        args.map_list(|arg|
        {
            arg.apply(lambdas, memory, env)
        })
    }

    fn apply_compound(
        lambdas: &Lambdas, 
        memory: &mut LispMemory,
        env: &mut Environment,
        proc: &CompoundProcedure,
        args: &Self
    ) -> Result<LispValue, Error>
    {
        let id = match proc
        {
            CompoundProcedure::Identifier(name) =>
            {
                let proc = env.lookup(name)?;

                proc.as_procedure()?
            },
            CompoundProcedure::Lambda(id) => *id
        };

        let proc = lambdas.get(id);

        let args = Self::apply_args(lambdas, memory, env, args)?;

        proc.apply(lambdas, memory, env, args)
    }

    fn eval(lambdas: &mut Lambdas, ast: Ast) -> Result<Self, Error>
    {
        if ast.is_list()
        {
            let op = Self::eval(lambdas, ast.car())?;

            let op = match op
            {
                Self::Value(name) => Procedure::from(name),
                Self::Lambda(id) => Procedure::Compound(CompoundProcedure::Lambda(id)),
                _ => return Err(Error::ExpectedOp)
            };

            let args = ast.cdr();

            Self::eval_nonatom(lambdas, op, args)
        } else
        {
            Self::eval_atom(ast)
        }
    }

    fn eval_nonatom(lambdas: &mut Lambdas, op: Procedure, args: Ast) -> Result<Self, Error>
    {
        let args = match op
        {
            Procedure::Primitive(p) =>
            {
                match p
                {
                    PrimitiveProcedure::Define =>
                    {
                        let first = args.car();
                        let body = args.cdr().car();
                        let is_procedure = first.is_list();

                        if is_procedure
                        {
                            let name = Self::ast_to_expression(first.car())?;
                            let name = Self::Value(name.as_value()?);

                            let params = first.cdr();

                            let lambda_args =
                                Ast::cons(
                                    params,
                                    Ast::cons(
                                        body,
                                        Ast::EmptyList));

                            let lambda = Self::eval_lambda(lambdas, lambda_args)?;

                            Self::cons(name, Self::cons(lambda, Self::EmptyList))
                        } else
                        {
                            Self::argument_count(2, &args)?;

                            Self::eval_args(lambdas, args)?
                        }
                    },
                    PrimitiveProcedure::Lambda => return Self::eval_lambda(lambdas, args),
                    PrimitiveProcedure::Quote =>
                    {
                        Self::argument_count(1, &args)?;

                        Self::ast_to_expression(args.car())?
                    },
                    _ => Self::eval_args(lambdas, args)?
                }
            },
            _ => Self::eval_args(lambdas, args)?
        };

        let args = Box::new(args);

        Ok(Self::Application{op, args})
    }

    fn eval_lambda(lambdas: &mut Lambdas, args: Ast) -> Result<Self, Error>
    {
        Self::argument_count(2, &args)?;

        let params = Self::ast_to_expression(args.car())?;
        let body = Self::eval(lambdas, args.cdr().car())?;

        let lambda = StoredLambda::new(params, body)?;

        let id = lambdas.add(lambda);

        return Ok(Self::Lambda(id));
    }

    fn ast_to_expression(ast: Ast) -> Result<Self, Error>
    {
        let out = match ast
        {
            Ast::Value(s) => Self::eval_primitive_ast(Ast::parse_primitive(&s)?),
            Ast::EmptyList => Self::EmptyList,
            Ast::List{car, cdr} => Self::List{
                car: Box::new(Self::ast_to_expression(*car)?),
                cdr: Box::new(Self::ast_to_expression(*cdr)?)
            }
        };

        Ok(out)
    }

    fn eval_args(lambdas: &mut Lambdas, args: Ast) -> Result<Self, Error>
    {
        if args.is_null()
        {
            return Ok(Self::EmptyList);
        }

        let out = Self::List{
            car: Box::new(Self::eval(lambdas, args.car())?),
            cdr: Box::new(Self::eval_args(lambdas, args.cdr())?)
        };

        Ok(out)
    }

    fn eval_primitive_ast(primitive: PrimitiveType) -> Self
    {
        match primitive
        {
            PrimitiveType::Value(x) => Self::Value(x),
            PrimitiveType::Float(x) => Self::Float(x),
            PrimitiveType::Integer(x) => Self::Integer(x)
        }
    }

    fn eval_atom(ast: Ast) -> Result<Self, Error>
    {
        Ok(Self::eval_primitive_ast(ast.as_value()?))
    }

    fn eval_sequence(lambdas: &mut Lambdas, ast: Ast) -> Result<Self, Error>
    {
        let car = Self::eval(lambdas, ast.car())?;
        let cdr = ast.cdr();

        Ok(if cdr.is_null()
        {
            car
        } else
        {
            Self::Sequence{
                first: Box::new(car),
                after: Box::new(Self::eval_sequence(lambdas, cdr)?)
            }
        })
    }

    fn argument_count(count: usize, args: &Ast) -> Result<(), Error>
    {
        if count < 1
        {
            return if args.is_null()
            {
                Ok(())
            } else
            {
                Err(Error::WrongArgumentsCount)
            };
        }

        Self::argument_count(count - 1, &args.cdr())
    }
}
