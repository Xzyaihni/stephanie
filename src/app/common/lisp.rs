use std::{
    mem,
    fmt::{self, Display, Debug},
    collections::HashMap
};

use program::Program;

mod program;


#[derive(Clone, Copy)]
pub union ValueRaw
{
    integer: i32,
    float: f32,
    procedure: usize,
    ptr: usize
}

#[derive(Debug, Clone, Copy)]
pub enum ValueTag
{
    Integer,
    Float,
    String,
    Symbol,
    True,
    False,
    EmptyList,
    Procedure,
    List,
    Vector
}

pub struct LispVector
{
    tag: ValueTag,
    values: Vec<ValueRaw>
}

impl LispVector
{
    pub fn as_vec_usize(self) -> Result<Vec<usize>, Error>
    {
        match self.tag
        {
            // eh
            ValueTag::Integer => Ok(self.values.into_iter().map(|x| 
            {
                unsafe{ x.integer as usize }
            }).collect()),
            x => Err(Error::WrongType(x))
        }
    }
}

#[derive(Clone, Copy)]
pub struct LispValue
{
    tag: ValueTag,
    value: ValueRaw
}

impl Debug for LispValue
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self.tag
        {
            ValueTag::Integer => unsafe{ self.value.integer.to_string() },
            ValueTag::Float => unsafe{ self.value.float.to_string() },
            ValueTag::True => "#t".to_owned(),
            ValueTag::False => "#f".to_owned(),
            ValueTag::String => unimplemented!(),
            ValueTag::Symbol => unimplemented!(),
            ValueTag::EmptyList => "()".to_owned(),
            ValueTag::Procedure => format!("<procedure #{}>", unsafe{ self.value.procedure }),
            ValueTag::List => unimplemented!(),
            ValueTag::Vector => unimplemented!()
        };

        write!(f, "{s}")
    }
}

impl LispValue
{
    // tag and value in the union must match
    pub unsafe fn new(tag: ValueTag, value: ValueRaw) -> Self
    {
        Self{tag, value}
    }

    pub fn new_procedure(procedure: usize) -> Self
    {
        unsafe{
            LispValue::new(ValueTag::Procedure, ValueRaw{procedure})
        }
    }

    pub fn new_bool(value: bool) -> Self
    {
        let tag = if value
        {
            ValueTag::True
        } else
        {
            ValueTag::False
        };

        unsafe{
            LispValue::new(tag, ValueRaw{integer: 0})
        }
    }

    pub fn new_integer(value: i32) -> Self
    {
        unsafe{
            LispValue::new(ValueTag::Integer, ValueRaw{integer: value})
        }
    }

    pub fn new_float(value: f32) -> Self
    {
        unsafe{
            LispValue::new(ValueTag::Float, ValueRaw{float: value})
        }
    }

    pub fn tag(&self) -> ValueTag
    {
        self.tag
    }

    pub fn is_true(&self) -> bool
    {
        match self.tag
        {
            ValueTag::False => false,
            _ => true
        }
    }

    pub fn as_vector(self) -> Result<LispVector, Error>
    {
        match self.tag
        {
            ValueTag::Vector => todo!(),// Ok(unsafe{ LispVector::from_raw(self.value) }),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_integer(self) -> Result<i32, Error>
    {
        match self.tag
        {
            ValueTag::Integer => Ok(unsafe{ self.value.integer }),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_float(self) -> Result<f32, Error>
    {
        match self.tag
        {
            ValueTag::Float => Ok(unsafe{ self.value.float }),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_procedure(self) -> Result<usize, Error>
    {
        match self.tag
        {
            ValueTag::Procedure => Ok(unsafe{ self.value.procedure }),
            x => Err(Error::WrongType(x))
        }
    }
}

#[derive(Debug, Clone)]
pub enum Error
{
    WrongType(ValueTag),
    NumberParse(String),
    UndefinedVariable(String),
    ApplyNonApplication,
    WrongArgumentsCount,
    ExpectedSameNumberType,
    ExpectedArg,
    ExpectedOp,
    ExpectedClose,
    UnexpectedClose
}

impl Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::WrongType(tag) => format!("wrong type `{tag:?}`"),
            Self::NumberParse(s) => format!("cant parse `{s}` as number"),
            Self::UndefinedVariable(s) => format!("variable `{s}` is undefined"),
            Self::ApplyNonApplication => "apply was called on a non application".to_owned(),
            Self::ExpectedSameNumberType => "primitive operation expected 2 numbers of same type".to_owned(),
            Self::WrongArgumentsCount => "wrong amount of arguments passed to procedure".to_owned(),
            Self::ExpectedArg => "expected an argument".to_owned(),
            Self::ExpectedOp => "expected an operator".to_owned(),
            Self::ExpectedClose => "expected a closing parenthesis".to_owned(),
            Self::UnexpectedClose => "unexpected closing parenthesis".to_owned()
        };

        write!(f, "{}", s)
    }
}

struct MemoryBlock
{
    cars: Vec<LispValue>,
    cdrs: Vec<LispValue>
}

impl MemoryBlock
{
    pub fn new(memory_size: usize) -> Self
    {
        let half_memory = memory_size / 2;
        let cars = Vec::with_capacity(half_memory);
        let cdrs = Vec::with_capacity(half_memory);

        Self{cars, cdrs}
    }

    pub fn remaining(&self) -> usize
    {
        self.cars.capacity() - self.cars.len()
    }

    pub fn clear(&mut self)
    {
        self.cars.clear();
        self.cdrs.clear();
    }
}

struct LispMemory
{
    memory: MemoryBlock,
    swap_memory: MemoryBlock,
}

impl LispMemory
{
    pub fn new(memory_size: usize) -> Self
    {
        let half_memory = memory_size / 2;
        let memory = MemoryBlock::new(half_memory);
        let swap_memory = MemoryBlock::new(half_memory);

        Self{memory, swap_memory}
    }

    fn gc(&mut self)
    {
        self.swap_memory.clear();

        mem::swap(&mut self.memory, &mut self.swap_memory);
    }

    pub fn cons(&mut self, car: LispValue, cdr: LispValue)
    {
        if self.memory.remaining() < 1
        {
            self.gc();
        }

        todo!();
    }
}

pub struct Environment<'a>
{
    parent: Option<&'a Self>,
    mappings: HashMap<String, LispValue>
}

impl<'a> Environment<'a>
{
    pub fn new() -> Self
    {
        let mappings = HashMap::new();

        Self{parent: None, mappings}
    }

    pub fn child(parent: &'a Self) -> Self
    {
        let mappings = HashMap::new();

        Self{parent: Some(parent), mappings}
    }

    pub fn define(&mut self, key: impl Into<String>, value: LispValue)
    {
        self.mappings.insert(key.into(), value);
    }

    pub fn try_lookup(&self, key: &str) -> Option<LispValue>
    {
        self.mappings.get(key).copied().or_else(||
        {
            self.parent.and_then(|parent|
            {
                parent.try_lookup(key)
            })
        })
    }

    pub fn lookup(&self, key: &str) -> Result<LispValue, Error>
    {
        self.try_lookup(key).ok_or_else(|| Error::UndefinedVariable(key.to_owned()))
    }
}

pub struct Lisp
{
    memory: LispMemory,
    program: Program
}

impl Lisp
{
    pub fn new(code: &str) -> Result<Self, Error>
    {
        let program = Program::parse(code)?;

        let memory_size = 1 << 10;
        let memory = LispMemory::new(memory_size);

        Ok(Self{program, memory})
    }

    pub fn run(&self) -> Result<LispValue, Error>
    {
        let mut env = Environment::new();

        self.program.apply(&mut env)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn ycomb_factorial()
    {
        let code = "
            ((lambda (x)
                    ((lambda (f) (f f x))
                        (lambda (f n)
                            (if (= n 1)
                                1
                                (* n (f f (- n 1)))))))
                7)
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5040_i32);
    }

    #[test]
    fn factorial()
    {
        let code = "
            (define (factorial n)
                (if (= n 1)
                    1
                    (* n (factorial (- n 1)))))

            (factorial 7)
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5040_i32);
    }

    #[test]
    fn if_test()
    {
        let code = "
            (define x
                (lambda (x)
                    (if (= x 1)
                        8
                        2)))

            (+ (x 1) (x 5))
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 10_i32);
    }

    #[test]
    fn define()
    {
        let code = "
            (define x (+ 2 3))

            x
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5_i32);
    }

    #[test]
    fn lambda_define()
    {
        let code = "
            (define x (lambda (value) (+ value value)))

            (x 1312)
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 1312_i32 * 2);
    }

    #[test]
    fn addition()
    {
        let code = "
            (+ 3 6)
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 9_i32);
    }

    #[test]
    fn multi_addition()
    {
        let code = "
            (+ 1 2 3)
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 6_i32);
    }

    #[test]
    fn weird_spacing_addition()
    {
        let code = "
            (+   1  2
              3

              )
        ";

        let lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 6_i32);
    }
}
