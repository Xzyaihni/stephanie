use std::{
    vec,
    iter,
    fmt::{self, Debug},
    sync::Arc,
    collections::HashMap,
    ops::{Add, Sub, Mul, Div, Rem, Deref, DerefMut}
};

pub use super::{Error, Environment, LispValue, LispMemory, ValueTag, LispVectorRef};
use parser::{Parser, Ast, PrimitiveType};

mod parser;


// unreadable, great
pub type OnApply = Arc<
    dyn Fn(
        &State,
        &mut LispMemory,
        &mut Environment,
        &Expression
    ) -> Result<LispValue, Error> + Send + Sync>;

pub type OnEval = Arc<
    dyn Fn(
        Option<OnApply>,
        &mut State,
        Ast
    ) -> Result<Expression, Error> + Send + Sync>;

#[derive(Clone)]
pub struct PrimitiveProcedureInfo
{
    args_count: Option<usize>,
    on_eval: Option<OnEval>,
    on_apply: Option<OnApply>
}

impl PrimitiveProcedureInfo
{
    pub fn new_eval(
        args_count: impl Into<Option<usize>>,
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
        args_count: impl Into<Option<usize>>,
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

    pub fn new_simple(args_count: impl Into<Option<usize>>, on_apply: OnApply) -> Self
    {
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
        let args_count = self.args_count.map(|x| x.to_string())
            .unwrap_or_else(|| "no".to_owned());

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

impl Procedure
{
    pub fn parse(state: &mut State, ast: Ast, s: String) -> Result<Expression, Error>
    {
        if let Some(primitive) = state.primitives.get(&s).cloned()
        {
            if let Some(count) = primitive.args_count
            {
                Expression::argument_count_ast(count, &ast)?;
            }

            if let Some(on_eval) = primitive.on_eval.as_ref()
            {
                on_eval(primitive.on_apply, state, ast)
            } else
            {
                let args = Expression::eval_args(state, ast)?;
                let p = PrimitiveProcedure(primitive.on_apply.expect("apply must be provided"));

                Ok(Expression::Application{
                    op: Self::Primitive(p),
                    args: Box::new(args)
                })
            }
        } else
        {
            let op = Self::Compound(CompoundProcedure::Identifier(s));

            Ok(Expression::Application{
                op,
                args: Box::new(Expression::eval_args(state, ast)?)
            })
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
        state: &State,
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

        self.body.apply(state, memory, &mut new_env)
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
pub struct Primitives(HashMap<String, PrimitiveProcedureInfo>);

impl Deref for Primitives
{
    type Target = HashMap<String, PrimitiveProcedureInfo>;

    fn deref(&self) -> &Self::Target
    {
        &self.0
    }
}

impl DerefMut for Primitives
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.0
    }
}

impl Primitives
{
    pub fn new() -> Self
    {
        macro_rules! do_cond
        {
            ($f:expr) =>
            {
                Arc::new(|state, memory, env, args|
                {
                    let args = Expression::apply_args(state, memory, env, args)?;

                    Self::do_cond(args, $f, $f)
                })
            }
        }

        macro_rules! do_op
        {
            ($op:ident) =>
            {
                Arc::new(|state, memory, env, args|
                {
                    let args = Expression::apply_args(state, memory, env, args)?;

                    Self::do_op(args, |a, b|
                    {
                        LispValue::new_integer(a.$op(b))
                    }, |a, b|
                    {
                        LispValue::new_float(a.$op(b))
                    })
                })
            }
        }

        macro_rules! is_tag
        {
            ($tag:expr) =>
            {
                Arc::new(|state, memory, env, args|
                {
                    let arg = args.car().apply(state, memory, env)?;

                    let is_equal = arg.tag == $tag;

                    Ok(LispValue::new_bool(is_equal))
                })
            }
        }

        let primitives = [
            ("make-vector", PrimitiveProcedureInfo::new_simple(2, Arc::new(|state, memory, env, args|
            {
                let mut args = Expression::apply_args(state, memory, env, args)?;

                let len = args.pop()?.as_integer()? as usize;
                let fill = args.pop()?;

                let vec = LispVectorRef{
                    tag: fill.tag,
                    values: &vec![fill.value; len]
                };

                Ok(LispValue::new_vector(memory.allocate_vector(vec)))
            }))),
            ("vector-set!", PrimitiveProcedureInfo::new_simple(3, Arc::new(|state, memory, env, args|
            {
                let mut args = Expression::apply_args(state, memory, env, args)?;

                let vec = args.pop()?.as_vector_mut(memory)?;
                let index = args.pop()?.as_integer()?;
                let value = args.pop()?;

                if vec.tag != value.tag
                {
                    return Err(Error::VectorWrongType{expected: vec.tag, got: value.tag});
                }

                Self::check_inbounds(&vec.values, index)?;

                vec.values[index as usize] = value.value;

                Ok(LispValue::new_empty_list())
            }))),
            ("vector-ref", PrimitiveProcedureInfo::new_simple(2, Arc::new(|state, memory, env, args|
            {
                let mut args = Expression::apply_args(state, memory, env, args)?;

                let vec = args.pop()?.as_vector_ref(memory)?;
                let index = args.pop()?.as_integer()?;

                Self::check_inbounds(&vec.values, index)?;

                let value = vec.values[index as usize];

                Ok(unsafe{ LispValue::new(vec.tag, value) })
            }))),
            ("symbol?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Symbol))),
            ("pair?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::List))),
            ("char?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Char))),
            ("vector?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Vector))),
            ("procedure?", PrimitiveProcedureInfo::new_simple(1, is_tag!(ValueTag::Procedure))),
            ("number?", PrimitiveProcedureInfo::new_simple(1, Arc::new(|state, memory, env, args|
            {
                let arg = args.car().apply(state, memory, env)?;

                let is_number = arg.tag == ValueTag::Integer || arg.tag == ValueTag::Float;

                Ok(LispValue::new_bool(is_number))
            }))),
            ("boolean?", PrimitiveProcedureInfo::new_simple(1, Arc::new(|state, memory, env, args|
            {
                let arg = args.car().apply(state, memory, env)?;

                let is_bool = arg.as_bool().map(|_| true).unwrap_or(false);

                Ok(LispValue::new_bool(is_bool))
            }))),
            ("+", PrimitiveProcedureInfo::new_simple(None, do_op!(add))),
            ("-", PrimitiveProcedureInfo::new_simple(None, do_op!(sub))),
            ("*", PrimitiveProcedureInfo::new_simple(None, do_op!(mul))),
            ("/", PrimitiveProcedureInfo::new_simple(None, do_op!(div))),
            ("remainder", PrimitiveProcedureInfo::new_simple(2, do_op!(rem))),
            ("=",
                PrimitiveProcedureInfo::new_simple(
                    None,
                    do_cond!(|a, b| LispValue::new_bool(a == b)))),
            (">",
                PrimitiveProcedureInfo::new_simple(
                    None,
                    do_cond!(|a, b| LispValue::new_bool(a > b)))),
            ("<",
                PrimitiveProcedureInfo::new_simple(
                    None,
                    do_cond!(|a, b| LispValue::new_bool(a < b)))),
            ("if",
                PrimitiveProcedureInfo::new_simple(3, Arc::new(|state, memory, env, args|
                {
                    let predicate = args.car().apply(state, memory, env)?;
                    let on_true = args.cdr().car();
                    let on_false = args.cdr().cdr().car();

                    if predicate.is_true()
                    {
                        on_true.apply(state, memory, env)
                    } else
                    {
                        on_false.apply(state, memory, env)
                    }
                }))),
            ("let",
                PrimitiveProcedureInfo::new_eval(2, Arc::new(|_on_apply, state, args|
                {
                    let bindings = args.car();
                    let body = args.cdr().car();

                    let params = bindings.map_list(|x| x.car());
                    let apply_args = Expression::eval_args(
                        state,
                        bindings.map_list(|x| x.cdr().car())
                    )?;

                    let lambda_args =
                        Ast::cons(
                            params,
                            Ast::cons(
                                body,
                                Ast::EmptyList));

                    let lambda = Expression::eval_lambda(state, lambda_args)?;

                    Ok(Expression::Application{
                        op: Procedure::Compound(CompoundProcedure::Lambda(lambda)),
                        args: Box::new(apply_args)
                    })
                }))),
            ("begin",
                PrimitiveProcedureInfo::new_eval(None, Arc::new(|_on_apply, state, args|
                {
                    Expression::eval_sequence(state, args)
                }))),
            ("lambda",
                PrimitiveProcedureInfo::new_eval(2, Arc::new(|_on_apply, state, args|
                {
                    Ok(Expression::Lambda(Expression::eval_lambda(state, args)?))
                }))),
            ("define",
                PrimitiveProcedureInfo::new(2, Arc::new(|on_apply, state, args|
                {
                    let first = args.car();
                    let body = args.cdr().car();
                    let is_procedure = first.is_list();

                    let args = if is_procedure
                    {
                        let name = Expression::ast_to_expression(first.car())?;
                        let name = Expression::Value(name.as_value()?);

                        let params = first.cdr();

                        let lambda_args =
                            Ast::cons(
                                params,
                                Ast::cons(
                                    body,
                                    Ast::EmptyList));

                        let lambda = Expression::eval_lambda(state, lambda_args)?;

                        Expression::cons(
                            name,
                            Expression::cons(
                                Expression::Lambda(lambda),
                                Expression::EmptyList))
                    } else
                    {
                        Expression::argument_count_ast(2, &args)?;

                        Expression::eval_args(state, args)?
                    };

                    Ok(Expression::new_application(on_apply, args))
                }), Arc::new(|state, memory, env, args|
                {
                    let first = args.car();
                    let second = args.cdr().car();

                    let key = first.as_value()?;
                    let value = second.apply(state, memory, env)?;

                    env.define(key, value);

                    Ok(LispValue::new_empty_list())
                }))),
            ("quote",
                PrimitiveProcedureInfo::new(1, Arc::new(|on_apply, _state, args|
                {
                    let arg = Expression::ast_to_expression(args.car())?;

                    Ok(Expression::new_application(on_apply, arg))
                }), Arc::new(|_state, memory, _env, args|
                {
                    Ok(memory.allocate_expression(args))
                }))),
            ("cons",
                PrimitiveProcedureInfo::new_simple(2, Arc::new(|state, memory, env, args|
                {
                    let mut args = Expression::apply_args(state, memory, env, args)?;

                    let car = args.pop()?;
                    let cdr = args.pop()?;

                    Ok(memory.cons(car, cdr))
                }))),
            ("car",
                PrimitiveProcedureInfo::new_simple(1, Arc::new(|state, memory, env, args|
                {
                    let value = args.car().apply(state, memory, env)?
                        .as_list(memory)?;

                    Ok(value.car)
                }))),
            ("cdr",
                PrimitiveProcedureInfo::new_simple(1, Arc::new(|state, memory, env, args|
                {
                    let value = args.car().apply(state, memory, env)?
                        .as_list(memory)?;

                    Ok(value.cdr)
                }))),
        ].into_iter().map(|(k, v)| (k.to_owned(), v)).collect();

        Self(primitives)
    }

    pub fn add(&mut self, name: impl Into<String>, procedure: PrimitiveProcedureInfo)
    {
        self.0.insert(name.into(), procedure);
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
    pub lambdas: Lambdas,
    pub primitives: Arc<Primitives>
}

#[derive(Debug, Clone)]
pub struct Program
{
    state: State,
    expression: Expression
}

impl Program
{
    pub fn parse(
        primitives: Arc<Primitives>,
        lambdas: Option<Lambdas>,
        code: &str
    ) -> Result<Self, Error>
    {
        let ast = Parser::parse(code)?;

        let mut state = State{
            lambdas: lambdas.unwrap_or_else(|| Lambdas::new()),
            primitives
        };

        let expression = Expression::eval_sequence(&mut state, ast)?;

        Ok(Self{state, expression})
    }

    pub fn lambdas(&self) -> &Lambdas
    {
        &self.state.lambdas
    }

    pub fn apply(
        &self,
        memory: &mut LispMemory,
        env: &mut Environment
    ) -> Result<LispValue, Error>
    {
        self.expression.apply(&self.state, memory, env)
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
    pub fn car(&self) -> &Self
    {
        match self
        {
            Self::List{car, ..} => car,
            x => panic!("car must be called on a list, called on {x:?}")
        }
    }

    pub fn cdr(&self) -> &Self
    {
        match self
        {
            Self::List{cdr, ..} => cdr,
            x => panic!("cdr must be called on a list, called on {x:?}")
        }
    }

    pub fn cons(car: Self, cdr: Self) -> Self
    {
        Self::List{car: Box::new(car), cdr: Box::new(cdr)}
    }

    pub fn is_null(&self) -> bool
    {
        match self
        {
            Self::EmptyList => true,
            _ => false
        }
    }

    pub fn as_value(&self) -> Result<String, Error>
    {
        match self
        {
            Self::Value(x) => Ok(x.clone()),
            _ => Err(Error::ExpectedOp)
        }
    }

    pub fn apply(
        &self,
        state: &State, 
        memory: &mut LispMemory,
        env: &mut Environment
    ) -> Result<LispValue, Error>
    {
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
                    Procedure::Compound(p) => Self::apply_compound(state, memory, env, p, args),
                    Procedure::Primitive(p) => p.0(state, memory, env, args)
                }
            },
            Self::Sequence{first, after} =>
            {
                first.apply(state, memory, env)?;

                after.apply(state, memory, env)
            },
            _ => Err(Error::ApplyNonApplication)
        }
    }

    pub fn map_list<T, F>(&self, mut f: F) -> Result<ArgValues<T>, Error>
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

    pub fn apply_args(
        state: &State, 
        memory: &mut LispMemory,
        env: &mut Environment,
        args: &Self
    ) -> Result<ArgValues, Error>
    {
        args.map_list(|arg|
        {
            arg.apply(state, memory, env)
        })
    }

    pub fn apply_compound(
        state: &State, 
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

        let proc = state.lambdas.get(id);

        let args = Self::apply_args(state, memory, env, args)?;

        proc.apply(state, memory, env, args)
    }

    pub fn eval(state: &mut State, ast: Ast) -> Result<Self, Error>
    {
        if ast.is_list()
        {
            let op = Self::eval(state, ast.car())?;

            let args = ast.cdr();
            Self::eval_op(state, op, args)
        } else
        {
            Self::eval_atom(ast)
        }
    }

    pub fn eval_op(state: &mut State, op: Self, ast: Ast) -> Result<Self, Error>
    {
        match op
        {
            Self::Value(name) => Procedure::parse(state, ast, name),
            Self::Lambda(id) =>
            {
                Ok(Self::Application{
                    op: Procedure::Compound(CompoundProcedure::Lambda(id)),
                    args: Box::new(Self::eval_args(state, ast)?)
                })
            },
            _ => Err(Error::ExpectedOp)
        }
    }

    pub fn eval_lambda(state: &mut State, args: Ast) -> Result<usize, Error>
    {
        Self::argument_count_ast(2, &args)?;

        let params = Self::ast_to_expression(args.car())?;
        let body = Self::eval(state, args.cdr().car())?;

        let lambda = StoredLambda::new(params, body)?;

        let id = state.lambdas.add(lambda);

        Ok(id)
    }

    pub fn ast_to_expression(ast: Ast) -> Result<Self, Error>
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

    pub fn eval_args(state: &mut State, args: Ast) -> Result<Self, Error>
    {
        if args.is_null()
        {
            return Ok(Self::EmptyList);
        }

        let out = Self::List{
            car: Box::new(Self::eval(state, args.car())?),
            cdr: Box::new(Self::eval_args(state, args.cdr())?)
        };

        Ok(out)
    }

    pub fn eval_primitive_ast(primitive: PrimitiveType) -> Self
    {
        match primitive
        {
            PrimitiveType::Value(x) => Self::Value(x),
            PrimitiveType::Float(x) => Self::Float(x),
            PrimitiveType::Integer(x) => Self::Integer(x)
        }
    }

    pub fn eval_atom(ast: Ast) -> Result<Self, Error>
    {
        Ok(Self::eval_primitive_ast(ast.as_value()?))
    }

    pub fn eval_sequence(state: &mut State, ast: Ast) -> Result<Self, Error>
    {
        let car = Self::eval(state, ast.car())?;
        let cdr = ast.cdr();

        Ok(if cdr.is_null()
        {
            car
        } else
        {
            Self::Sequence{
                first: Box::new(car),
                after: Box::new(Self::eval_sequence(state, cdr)?)
            }
        })
    }

    pub fn new_application(on_apply: Option<OnApply>, expr: Self) -> Self
    {
        let op =
            Procedure::Primitive(PrimitiveProcedure(on_apply.expect("apply must be provided")));

        Expression::Application{
            op,
            args: Box::new(expr)
        }
    }

    pub fn argument_count(count: usize, args: &Self) -> Result<(), Error>
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

    pub fn argument_count_ast(count: usize, args: &Ast) -> Result<(), Error>
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

        Self::argument_count_ast(count - 1, &args.cdr())
    }
}
