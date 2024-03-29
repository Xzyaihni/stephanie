use std::{
    ops::Range,
    fmt::{self, Display, Debug},
    collections::HashMap
};

use program::{Program, Expression};

mod program;


#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum Special
{
    True,
    False,
    EmptyList
}

impl Special
{
    pub fn new_bool(value: bool) -> Self
    {
        if value
        {
            Self::True
        } else
        {
            Self::False
        }
    }

    pub fn new_empty_list() -> Self
    {
        Self::EmptyList
    }

    pub fn is_true(&self) -> bool
    {
        match self
        {
            Self::False => false,
            _ => true
        }
    }

    pub fn as_bool(&self) -> Option<bool>
    {
        match self
        {
            Self::True => Some(true),
            Self::False => Some(false),
            _ => None
        }
    }
}

impl Display for Special
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::True => "#t",
            Self::False => "#f",
            Self::EmptyList => "()"
        };

        write!(f, "{}", s)
    }
}

#[derive(Clone, Copy)]
pub union ValueRaw
{
    integer: i32,
    float: f32,
    char: char,
    len: usize,
    procedure: usize,
    tag: ValueTag,
    special: Special,
    list: usize,
    symbol: usize,
    string: usize,
    vector: usize
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueTag
{
    Integer,
    Float,
    Char,
    String,
    Symbol,
    Special,
    Procedure,
    List,
    Vector
}

pub struct LispVectorInner<T>
{
    tag: ValueTag,
    values: T
}

pub type LispVector = LispVectorInner<Vec<ValueRaw>>;
pub type LispVectorRef<'a> = LispVectorInner<&'a [ValueRaw]>;
pub type LispVectorMut<'a> = LispVectorInner<&'a mut [ValueRaw]>;

impl LispVector
{
    // eh
    pub fn as_vec_usize(self) -> Result<Vec<usize>, Error>
    {
        match self.tag
        {
            ValueTag::Integer => Ok(self.values.into_iter().map(|x| 
            {
                unsafe{ x.integer as usize }
            }).collect()),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_vec_integer(self) -> Result<Vec<i32>, Error>
    {
        match self.tag
        {
            ValueTag::Integer => Ok(self.values.into_iter().map(|x| 
            {
                unsafe{ x.integer }
            }).collect()),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_vec_char(self) -> Result<Vec<char>, Error>
    {
        match self.tag
        {
            ValueTag::Char => Ok(self.values.into_iter().map(|x| 
            {
                unsafe{ x.char }
            }).collect()),
            x => Err(Error::WrongType(x))
        }
    }
}

pub struct LispList
{
    car: LispValue,
    cdr: LispValue
}

impl LispList
{
    pub fn car(&self) -> &LispValue
    {
        &self.car
    }

    pub fn cdr(&self) -> &LispValue
    {
        &self.cdr
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
            ValueTag::Char => unsafe{ self.value.char.to_string() },
            ValueTag::Special => unsafe{ self.value.special.to_string() },
            ValueTag::String => unimplemented!(),
            ValueTag::Symbol => unimplemented!(),
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

    pub fn new_list(list: usize) -> Self
    {
        unsafe{
            Self::new(ValueTag::List, ValueRaw{list})
        }
    }

    pub fn new_vector(vector: usize) -> Self
    {
        unsafe{
            Self::new(ValueTag::Vector, ValueRaw{vector})
        }
    }

    pub fn new_string(string: usize) -> Self
    {
        unsafe{
            Self::new(ValueTag::String, ValueRaw{string})
        }
    }

    pub fn new_symbol(symbol: usize) -> Self
    {
        unsafe{
            Self::new(ValueTag::Symbol, ValueRaw{symbol})
        }
    }

    pub fn new_procedure(procedure: usize) -> Self
    {
        unsafe{
            Self::new(ValueTag::Procedure, ValueRaw{procedure})
        }
    }

    pub fn new_bool(value: bool) -> Self
    {
        Self::new_special(Special::new_bool(value))
    }

    pub fn new_empty_list() -> Self
    {
        Self::new_special(Special::new_empty_list())
    }

    fn new_special(special: Special) -> Self
    {
        unsafe{
            Self::new(ValueTag::Special, ValueRaw{special})
        }
    }

    pub fn new_integer(value: i32) -> Self
    {
        unsafe{
            Self::new(ValueTag::Integer, ValueRaw{integer: value})
        }
    }

    pub fn new_float(value: f32) -> Self
    {
        unsafe{
            Self::new(ValueTag::Float, ValueRaw{float: value})
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
            ValueTag::Special => unsafe{ self.value.special.is_true() },
            _ => true
        }
    }

    pub fn as_symbol(self, memory: &LispMemory) -> Result<String, Error>
    {
        match self.tag
        {
            ValueTag::Symbol => memory.get_symbol(unsafe{ self.value.symbol }),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_list(self, memory: &LispMemory) -> Result<LispList, Error>
    {
        match self.tag
        {
            ValueTag::List => Ok(memory.get_list(unsafe{ self.value.list })),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_vector_ref(self, memory: &LispMemory) -> Result<LispVectorRef, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector_ref(unsafe{ self.value.vector })),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_vector_mut(self, memory: &mut LispMemory) -> Result<LispVectorMut, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector_mut(unsafe{ self.value.vector })),
            x => Err(Error::WrongType(x))
        }
    }

    pub fn as_vector(self, memory: &LispMemory) -> Result<LispVector, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector(unsafe{ self.value.vector })),
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

    pub fn as_bool(self) -> Result<bool, Error>
    {
        match self.tag
        {
            ValueTag::Special =>
            {
                let special = unsafe{ self.value.special };

                special.as_bool().ok_or_else(|| Error::WrongType(ValueTag::Special))
            },
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
    IndexOutOfRange(i32),
    CharOutOfRange,
    VectorWrongType{expected: ValueTag, got: ValueTag},
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
            Self::IndexOutOfRange(i) => format!("index {i} out of range"),
            Self::CharOutOfRange => "char out of range".to_owned(),
            Self::VectorWrongType{expected, got} =>
                format!("vector expected `{expected:?}` got `{got:?}`"),
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
    general: Vec<ValueRaw>,
    cars: Vec<LispValue>,
    cdrs: Vec<LispValue>
}

impl MemoryBlock
{
    pub fn new(memory_size: usize) -> Self
    {
        let half_memory = memory_size / 2;

        let general = Vec::with_capacity(half_memory);

        let half_half = half_memory / 2;

        let cars = Vec::with_capacity(half_half);
        let cdrs = Vec::with_capacity(half_half);

        Self{general, cars, cdrs}
    }

    fn vector_info(&self, id: usize) -> (ValueTag, Range<usize>)
    {
        let len = unsafe{ self.general[id].len };
        let tag = unsafe{ self.general[id + 1].tag };

        let start = id + 2;
        (tag, start..(start + len))
    }

    pub fn get_vector_ref(&self, id: usize) -> LispVectorRef
    {
        let (tag, range) = self.vector_info(id);

        LispVectorRef{
            tag,
            values: &self.general[range]
        }
    }

    pub fn get_vector_mut(&mut self, id: usize) -> LispVectorMut
    {
        let (tag, range) = self.vector_info(id);

        LispVectorMut{
            tag,
            values: &mut self.general[range]
        }
    }

    pub fn get_vector(&self, id: usize) -> LispVector
    {
        let (tag, range) = self.vector_info(id);

        LispVector{
            tag,
            values: self.general[range].to_vec()
        }
    }

    pub fn get_symbol(&self, id: usize) -> Result<String, Error>
    {
        let vec = self.get_vector(id);

        if vec.tag != ValueTag::Char
        {
            return Err(Error::WrongType(vec.tag));
        }

        Ok(vec.values.into_iter().map(|x| unsafe{ x.char }).collect())
    }

    pub fn get_list(&self, id: usize) -> LispList
    {
        LispList{
            car: self.get_car(id),
            cdr: self.get_cdr(id)
        }
    }

    pub fn get_car(&self, id: usize) -> LispValue
    {
        self.cars[id]
    }

    pub fn get_cdr(&self, id: usize) -> LispValue
    {
        self.cdrs[id]
    }

    pub fn cons(&mut self, car: LispValue, cdr: LispValue) -> LispValue
    {
        let id = self.cars.len();

        debug_assert!(self.cdrs.len() == id);

        self.cars.push(car);
        self.cdrs.push(cdr);

        LispValue::new_list(id)
    }

    fn allocate_iter(
        &mut self,
        len: usize,
        tag: ValueTag,
        iter: impl Iterator<Item=ValueRaw>
    ) -> usize
    {
        let id = self.general.len();

        let iter = [ValueRaw{len}, ValueRaw{tag}].into_iter().chain(iter);
        self.general.extend(iter);

        id
    }

    fn remaining_of<T>(v: &Vec<T>) -> usize
    {
        v.capacity() - v.len()
    }

    pub fn remaining(&self) -> usize
    {
        Self::remaining_of(&self.general)
    }

    pub fn list_remaining(&self) -> usize
    {
        Self::remaining_of(&self.cars)
    }

    pub fn clear(&mut self)
    {
        self.cars.clear();
        self.cdrs.clear();
    }
}

pub struct LispMemory
{
    memory: MemoryBlock
}

impl LispMemory
{
    pub fn new(memory_size: usize) -> Self
    {
        let memory = MemoryBlock::new(memory_size);

        Self{memory}
    }

    pub fn clear(&mut self)
    {
        self.memory.clear();
    }

    fn gc(&mut self)
    {
        panic!("due to my bespoke decisions gc is too hard to implement :( out of memory btw");
    }

    pub fn get_vector_ref(&self, id: usize) -> LispVectorRef
    {
        self.memory.get_vector_ref(id)
    }

    pub fn get_vector_mut(&mut self, id: usize) -> LispVectorMut
    {
        self.memory.get_vector_mut(id)
    }

    pub fn get_vector(&self, id: usize) -> LispVector
    {
        self.memory.get_vector(id)
    }

    pub fn get_symbol(&self, id: usize) -> Result<String, Error>
    {
        self.memory.get_symbol(id)
    }

    pub fn get_list(&self, id: usize) -> LispList
    {
        self.memory.get_list(id)
    }

    #[allow(dead_code)]
    pub fn get_car(&self, id: usize) -> LispValue
    {
        self.memory.get_car(id)
    }

    #[allow(dead_code)]
    pub fn get_cdr(&self, id: usize) -> LispValue
    {
        self.memory.get_car(id)
    }

    fn need_list_memory(&mut self, amount: usize)
    {
        if self.memory.list_remaining() < amount
        {
            self.gc();
        }
    }

    fn need_memory(&mut self, amount: usize)
    {
        if self.memory.remaining() < amount
        {
            self.gc();
        }
    }

    pub fn cons(&mut self, car: LispValue, cdr: LispValue) -> LispValue
    {
        self.need_list_memory(1);

        self.memory.cons(car, cdr)
    }

    pub fn allocate_vector(&mut self, vec: LispVector) -> usize
    {
        let len = vec.values.len();

        // +2 for the length and for the type tag
        self.need_memory(len + 2);

        self.memory.allocate_iter(len, vec.tag, vec.values.into_iter())
    }

    pub fn allocate_expression(&mut self, expression: &Expression) -> LispValue
    {
        match expression
        {
            Expression::Float(x) => LispValue::new_float(*x),
            Expression::Integer(x) => LispValue::new_integer(*x),
            Expression::EmptyList => LispValue::new_empty_list(),
            Expression::Lambda(x) => LispValue::new_procedure(*x),
            Expression::List{car, cdr} =>
            {
                let car = self.allocate_expression(car);
                let cdr = self.allocate_expression(cdr);

                self.cons(car, cdr)
            },
            Expression::Value(x) =>
            {
                let vec = LispVector{
                    tag: ValueTag::Char,
                    values: x.chars().map(|c| ValueRaw{char: c}).collect()
                };

                let id = self.allocate_vector(vec);

                LispValue::new_symbol(id)
            },
            Expression::Application{..} => unreachable!(),
            Expression::Sequence{..} => unreachable!()
        }
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
        let memory_size = 1 << 10;
        let memory = LispMemory::new(memory_size);

        Self::new_with_memory(memory, code)
    }

    pub fn new_with_memory(memory: LispMemory, code: &str) -> Result<Self, Error>
    {
        let program = Program::parse(code)?;

        Ok(Self{program, memory})
    }

    pub fn memory(&self) -> &LispMemory
    {
        &self.memory
    }

    pub fn run(&mut self) -> Result<LispValue, Error>
    {
        let mut env = Environment::new();

        self.program.apply(&mut self.memory, &mut env)
    }

    pub fn get_symbol(&self, value: LispValue) -> Result<String, Error>
    {
        value.as_symbol(&self.memory)
    }

    pub fn get_vector(&self, value: LispValue) -> Result<LispVector, Error>
    {
        value.as_vector(&self.memory)
    }

    pub fn get_list(&self, value: LispValue) -> Result<LispList, Error>
    {
        value.as_list(&self.memory)
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

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5040_i32);
    }

    #[test]
    fn equalities()
    {
        let code = "
            (define (test x y z)
                (if (= x y z)
                    7
                    1))

            (+ (test 1 2 3) (test 7 2 1) (test 3 2 2) (test 3 3 3))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 10_i32);
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

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5040_i32);
    }

    #[test]
    fn let_thingy()
    {
        let code = "
            (let ((x (+ 2 1)) (y (* 4 7)))
                (let ((z (+ x y)))
                    (+ z y x)))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        // simple math too hard for me sry
        assert_eq!(value, (34_i32 + 28));
    }

    #[test]
    fn begin()
    {
        let code = "
            (define v (make-vector 3 123))

            (vector-set!
                (begin
                    (vector-set! v 0 1)
                    (vector-set! v 1 2)
                    v)
                2
                3)

            v
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_vector(output).unwrap().as_vec_integer().unwrap();

        assert_eq!(value, vec![1, 2, 3]);
    }

    #[test]
    fn uses_lots_of_memory_or_something_lol()
    {
        let code = "
            (define (factorial-list n)
                (if (= n 1)
                    (quote (1))
                    (let ((next (factorial-list (- n 1))))
                        (let ((this (* n (car next))))
                            (cons this next)))))

            (define (silly x) (car (factorial-list x)))

            (+ (silly 7) (silly 5) (silly 6) (silly 11) (silly 4))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 39_922_704_i32);
    }

    fn list_equals(memory: &LispMemory, list: LispList, check: &[i32])
    {
        let car = list.car().as_integer().unwrap();

        assert_eq!(car, check[0]);

        let check = &check[1..];
        if check.is_empty()
        {
            return;
        }

        list_equals(memory, list.cdr().as_list(memory).unwrap(), check)
    }

    #[test]
    fn quoting()
    {
        let code = "
            (quote (1 2 3 4 5))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_list(output).unwrap();

        list_equals(lisp.memory(), value, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn list()
    {
        let code = "
            (cons 3 (cons 4 (cons 5 (quote ()))))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_list(output).unwrap();

        list_equals(lisp.memory(), value, &[3, 4, 5]);
    }

    #[test]
    fn carring()
    {
        let code = "
            (define x (cons 3 (cons 4 (cons 5 (quote ())))))

            (car (cdr (cdr x)))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 5_i32);
    }

    #[test]
    fn symbols()
    {
        // hmm
        let code = "
            (define x (quote (bratty lisp 💢 correction needed)))

            (car (cdr (cdr x)))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_symbol(output).unwrap();

        assert_eq!(value, "💢".to_owned());
    }

    #[test]
    fn make_vector()
    {
        let code = "
            (make-vector 5 999)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_vector(output).unwrap().as_vec_integer().unwrap();

        assert_eq!(value, vec![999, 999, 999, 999, 999]);
    }

    #[test]
    fn manip_vector()
    {
        let code = "
            (define x (make-vector 5 999))

            (vector-set! x 3 123)
            (vector-set! x 2 5)
            (vector-set! x 1 9)
            (vector-set! x 4 1000)

            (vector-set!
                x
                0
                (+ (vector-ref x 2) (vector-ref x 4)))

            x
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();
        let value = lisp.get_vector(output).unwrap().as_vec_integer().unwrap();

        assert_eq!(value, vec![1005, 9, 5, 123, 1000]);
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

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 10_i32);
    }

    #[test]
    fn predicates_stuff()
    {
        let code = "
            (define (x a)
                (if a
                    1
                    0))

            (+
                (x (boolean? (= 2 3)))
                (x (pair? (quote (1 2 3))))
                (x (number? (quote abcdefg))))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 2_i32);
    }

    #[test]
    fn define()
    {
        let code = "
            (define x (+ 2 3))

            x
        ";

        let mut lisp = Lisp::new(code).unwrap();

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

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 1312_i32 * 2);
    }

    #[test]
    fn addition()
    {
        let code = "
            (+ 3 6)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 9_i32);
    }

    #[test]
    fn multi_addition()
    {
        let code = "
            (+ 1 2 3)
        ";

        let mut lisp = Lisp::new(code).unwrap();

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

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 6_i32);
    }
}
