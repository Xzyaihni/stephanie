use std::{
    mem,
    cell::RefCell,
    sync::Arc,
    ops::Range,
    fmt::{self, Display, Debug},
    ops::{Deref, DerefMut},
    collections::HashMap
};

use parking_lot::Mutex;

pub use program::{PrimitiveProcedureInfo, Primitives, Lambdas, WithPosition};
use program::{Program, Expression, CodePosition};

mod program;


#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum Special
{
    True,
    False,
    EmptyList,
    BrokenHeart
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

    pub fn new_broken_heart() -> Self
    {
        Self::BrokenHeart
    }

    pub fn is_broken_heart(&self) -> bool
    {
        match self
        {
            Self::BrokenHeart => true,
            _ => false
        }
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
            Self::EmptyList => "()",
            Self::BrokenHeart => "<broken-heart>"
        };

        write!(f, "{}", s)
    }
}

#[derive(Clone, Copy)]
pub union ValueRaw
{
    unsigned: u32,
    pub integer: i32,
    pub float: f32,
    pub char: char,
    pub len: usize,
    pub procedure: usize,
    tag: ValueTag,
    pub special: Special,
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

impl ValueTag
{
    pub fn is_boxed(self) -> bool
    {
        match self
        {
            ValueTag::Integer
                | ValueTag::Float
                | ValueTag::Char
                | ValueTag::Procedure
                | ValueTag::Special => false,
            ValueTag::String
                | ValueTag::Symbol
                | ValueTag::List
                | ValueTag::Vector => true
        }
    }
}

pub struct LispVectorInner<T>
{
    pub tag: ValueTag,
    pub values: T
}

pub type LispVector = LispVectorInner<Vec<ValueRaw>>;
pub type LispVectorRef<'a> = LispVectorInner<&'a [ValueRaw]>;
pub type LispVectorMut<'a> = LispVectorInner<&'a mut [ValueRaw]>;

impl<T: IntoIterator<Item=ValueRaw>> LispVectorInner<T>
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
            x => Err(Error::VectorWrongType{expected: ValueTag::Integer, got: x})
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
            x => Err(Error::VectorWrongType{expected: ValueTag::Integer, got: x})
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
        let s = self.maybe_to_string(None).unwrap_or_else(|| "<value>".to_owned());

        write!(f, "{s}")
    }
}

impl LispValue
{
    /// # Safety
    /// tag and value in the union must match
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

    pub fn new_broken_heart() -> Self
    {
        Self::new_special(Special::new_broken_heart())
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

    pub fn is_broken_heart(&self) -> bool
    {
        self.as_special().map(|x| x.is_broken_heart()).unwrap_or(false)
    }

    pub fn is_true(&self) -> bool
    {
        self.as_special().map(|x| x.is_true()).unwrap_or(true)
    }

    fn as_special(&self) -> Option<Special>
    {
        match self.tag
        {
            ValueTag::Special => Some(unsafe{ self.value.special }),
            _ => None
        }
    }

    pub fn as_symbol(self, memory: &LispMemory) -> Result<String, Error>
    {
        match self.tag
        {
            ValueTag::Symbol => memory.get_symbol(unsafe{ self.value.symbol }),
            x => Err(Error::WrongType{expected: ValueTag::Symbol, got: x})
        }
    }

    pub fn as_list(self, memory: &LispMemory) -> Result<LispList, Error>
    {
        match self.tag
        {
            ValueTag::List => Ok(memory.get_list(unsafe{ self.value.list })),
            x => Err(Error::WrongType{expected: ValueTag::List, got: x})
        }
    }

    pub fn as_vector_ref(self, memory: &LispMemory) -> Result<LispVectorRef, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector_ref(unsafe{ self.value.vector })),
            x => Err(Error::WrongType{expected: ValueTag::Vector, got: x})
        }
    }

    pub fn as_vector_mut(self, memory: &mut LispMemory) -> Result<LispVectorMut, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector_mut(unsafe{ self.value.vector })),
            x => Err(Error::WrongType{expected: ValueTag::Vector, got: x})
        }
    }

    pub fn as_vector(self, memory: &LispMemory) -> Result<LispVector, Error>
    {
        match self.tag
        {
            ValueTag::Vector => Ok(memory.get_vector(unsafe{ self.value.vector })),
            x => Err(Error::WrongType{expected: ValueTag::Vector, got: x})
        }
    }

    pub fn as_integer(self) -> Result<i32, Error>
    {
        match self.tag
        {
            ValueTag::Integer => Ok(unsafe{ self.value.integer }),
            x => Err(Error::WrongType{expected: ValueTag::Integer, got: x})
        }
    }

    pub fn as_float(self) -> Result<f32, Error>
    {
        match self.tag
        {
            ValueTag::Float => Ok(unsafe{ self.value.float }),
            x => Err(Error::WrongType{expected: ValueTag::Float, got: x})
        }
    }

    pub fn as_bool(self) -> Result<bool, Error>
    {
        match self.tag
        {
            ValueTag::Special =>
            {
                let special = unsafe{ self.value.special };

                special.as_bool().ok_or(Error::WrongSpecial{expected: "boolean"})
            },
            x => Err(Error::WrongType{expected: ValueTag::Special, got: x})
        }
    }

    pub fn as_procedure(self) -> Result<usize, Error>
    {
        match self.tag
        {
            ValueTag::Procedure => Ok(unsafe{ self.value.procedure }),
            x => Err(Error::WrongType{expected: ValueTag::Procedure, got: x})
        }
    }

    pub fn to_string(&self, memory: &LispMemory) -> String
    {
        self.maybe_to_string(Some(memory)).expect("always returns some with memory")
    }

    fn maybe_to_string(&self, memory: Option<&LispMemory>) -> Option<String>
    {
        match self.tag
        {
            ValueTag::Integer => Some(unsafe{ self.value.integer.to_string() }),
            ValueTag::Float => Some(unsafe{ self.value.float.to_string() }),
            ValueTag::Char => Some(unsafe{ self.value.char.to_string() }),
            ValueTag::Special => Some(unsafe{ self.value.special.to_string() }),
            ValueTag::Procedure => Some(format!("<procedure #{}>", unsafe{ self.value.procedure })),
            ValueTag::String => memory.map(|memory|
            {
                memory.get_string(unsafe{ self.value.string }).unwrap()
            }),
            ValueTag::Symbol => memory.map(|memory|
            {
                let s = memory.get_symbol(unsafe{ self.value.symbol }).unwrap();

                format!("'{s}")
            }),
            ValueTag::List => memory.map(|memory|
            {
                let list = memory.get_list(unsafe{ self.value.list });

                let car = list.car.to_string(memory);
                let cdr = list.cdr.to_string(memory);

                format!("({car} {cdr})")
            }),
            ValueTag::Vector => memory.map(|memory|
            {
                let vec = memory.get_vector_ref(unsafe{ self.value.vector });

                let mut s = vec.values.iter().map(|raw| unsafe{ LispValue::new(vec.tag, *raw) })
                    .fold("#(".to_owned(), |acc, value|
                    {
                        acc + " " + &value.to_string(memory)
                    });

                s.push(')');

                s
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorPos
{
    pub position: CodePosition,
    pub error: Error
}

impl Display for ErrorPos
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}: {}", self.position, self.error)
    }
}

#[derive(Debug, Clone)]
pub enum Error
{
    WrongType{expected: ValueTag, got: ValueTag},
    WrongSpecial{expected: &'static str},
    NumberParse(String),
    UndefinedVariable(String),
    ApplyNonApplication,
    WrongArgumentsCount{proc: String, expected: usize, got: usize},
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
            Self::WrongType{expected, got} => format!("expected type `{expected:?}` got `{got:?}`"),
            Self::WrongSpecial{expected} => format!("wrong special, expected `{expected:?}`"),
            Self::NumberParse(s) => format!("cant parse `{s}` as number"),
            Self::UndefinedVariable(s) => format!("variable `{s}` is undefined"),
            Self::ApplyNonApplication => "apply was called on a non application".to_owned(),
            Self::ExpectedSameNumberType => "primitive operation expected 2 numbers of same type".to_owned(),
            Self::WrongArgumentsCount{proc, expected, got} =>
                format!("wrong amount of arguments (got {got}) passed to {proc} (expected {expected})"),
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

impl Debug for MemoryBlock
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        // all fields r at least 32 bits long and u32 has no invalid states
        let general = self.general.iter().map(|raw| unsafe{ raw.unsigned }).collect::<Vec<u32>>();

        f.debug_struct("MemoryBlock")
            .field("cars", &self.cars)
            .field("cdrs", &self.cdrs)
            .field("general", &general)
            .finish()
    }
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
        self.get_string(id)
    }

    pub fn get_string(&self, id: usize) -> Result<String, Error>
    {
        let vec = self.get_vector_ref(id);

        if vec.tag != ValueTag::Char
        {
            return Err(Error::VectorWrongType{expected: ValueTag::Char, got: vec.tag});
        }

        Ok(vec.values.iter().map(|x| unsafe{ x.char }).collect())
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

    pub fn set_car(&mut self, id: usize, value: LispValue)
    {
        self.cars[id] = value;
    }

    pub fn set_cdr(&mut self, id: usize, value: LispValue)
    {
        self.cdrs[id] = value;
    }

    pub fn cons(&mut self, car: LispValue, cdr: LispValue) -> LispValue
    {
        let id = self.cars.len();

        debug_assert!(self.cdrs.len() == id);

        self.cars.push(car);
        self.cdrs.push(cdr);

        LispValue::new_list(id)
    }

    fn allocate_iter<'a>(
        &mut self,
        len: usize,
        tag: ValueTag,
        iter: impl Iterator<Item=&'a ValueRaw>
    ) -> usize
    {
        let id = self.general.len();

        let beginning = [ValueRaw{len}, ValueRaw{tag}];
        let iter = beginning.into_iter().chain(iter.copied());

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
        self.general.clear();
        self.cars.clear();
        self.cdrs.clear();
    }
}

pub struct LispMemory
{
    stack_size: usize,
    memory: MemoryBlock,
    swap_memory: MemoryBlock,
    returns: Vec<LispValue>
}

impl LispMemory
{
    pub fn new(memory_size: usize) -> Self
    {
        let stack_size = 256;
        let memory = MemoryBlock::new(memory_size);
        let swap_memory = MemoryBlock::new(memory_size);

        Self{stack_size, memory, swap_memory, returns: Vec::with_capacity(stack_size)}
    }

    pub fn clear(&mut self)
    {
        self.memory.clear();
    }

    fn transfer_to_swap_value(
        memory: &mut MemoryBlock,
        swap_memory: &mut MemoryBlock,
        value: LispValue
    ) -> LispValue
    {
        match value.tag
        {
            x if !x.is_boxed() => value,
            ValueTag::List =>
            {
                let id = unsafe{ value.value.list };

                let list = memory.get_list(id);

                if list.car.is_broken_heart()
                {
                    return list.cdr;
                }

                let new_car = Self::transfer_to_swap_value(memory, swap_memory, list.car);
                let new_cdr = Self::transfer_to_swap_value(memory, swap_memory, list.cdr);

                let output = swap_memory.cons(new_car, new_cdr);

                memory.set_car(id, LispValue::new_broken_heart());
                memory.set_cdr(id, output);

                output
            },
            ValueTag::Symbol | ValueTag::Vector | ValueTag::String =>
            {
                // doesnt do broken hearts stuff so cycles will blow up memory
                // but wutever!
                let id = unsafe{ value.value.vector };

                let (tag, range) = memory.vector_info(id);

                if tag.is_boxed()
                {
                    for index in range.clone()
                    {
                        let lisp_value = unsafe{ LispValue::new(tag, memory.general[index]) };

                        let new_value =
                            Self::transfer_to_swap_value(memory, swap_memory, lisp_value);

                        memory.general[index] = new_value.value;
                    }
                }

                let s = &memory.general[range];

                let id = swap_memory.allocate_iter(s.len(), tag, s.iter());

                match value.tag
                {
                    ValueTag::Symbol => LispValue::new_symbol(id),
                    ValueTag::String => LispValue::new_string(id),
                    ValueTag::Vector => LispValue::new_vector(id),
                    _ => unreachable!()
                }
            },
            _ => unreachable!()
        }
    }

    fn transfer_to_swap_env(&mut self, env: &Environment)
    {
        if let Some(parent) = env.parent
        {
            self.transfer_to_swap_env(parent);
        }

        env.mappings.borrow_mut().iter_mut().for_each(|(_key, value)|
        {
            *value = Self::transfer_to_swap_value(&mut self.memory, &mut self.swap_memory, *value);
        });
    }

    fn transfer_to_swap_returns(&mut self)
    {
        self.returns.iter_mut().for_each(|value|
        {
            *value = Self::transfer_to_swap_value(&mut self.memory, &mut self.swap_memory, *value);
        });
    }

    fn gc(&mut self, env: &Environment)
    {
        self.swap_memory.clear();

        self.transfer_to_swap_env(env);
        self.transfer_to_swap_returns();

        mem::swap(&mut self.memory, &mut self.swap_memory);
    }

    pub fn push_return(&mut self, value: LispValue)
    {
        if self.returns.len() >= self.stack_size
        {
            panic!("stack overflow!!!! ahhhh!!");
        }

        self.returns.push(value);
    }

    pub fn pop_return(&mut self) -> LispValue
    {
        self.returns.pop().expect("cant pop from an empty stack, how did this happen?")
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

    pub fn get_string(&self, id: usize) -> Result<String, Error>
    {
        self.memory.get_string(id)
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

    fn need_list_memory(&mut self, env: &Environment, amount: usize)
    {
        if self.memory.list_remaining() < amount
        {
            self.gc(env);

            if self.memory.list_remaining() < amount
            {
                panic!("out of memory");
            }
        }
    }

    fn need_memory(&mut self, env: &Environment, amount: usize)
    {
        if self.memory.remaining() < amount
        {
            self.gc(env);

            if self.memory.remaining() < amount
            {
                panic!("out of memory");
            }
        }
    }

    pub fn cons(
        &mut self,
        env: &Environment,
        car: LispValue,
        cdr: LispValue
    ) -> LispValue
    {
        self.need_list_memory(env, 1);

        self.memory.cons(car, cdr)
    }

    pub fn allocate_vector(
        &mut self,
        env: &Environment,
        vec: LispVectorInner<&[ValueRaw]>
    ) -> usize
    {
        let len = vec.values.len();

        // +2 for the length and for the type tag
        self.need_memory(env, len + 2);

        self.memory.allocate_iter(len, vec.tag, vec.values.iter())
    }

    pub fn allocate_expression(
        &mut self,
        env: &Environment,
        expression: &Expression
    ) -> LispValue
    {
        match expression
        {
            Expression::Float(x) => LispValue::new_float(*x),
            Expression::Integer(x) => LispValue::new_integer(*x),
            Expression::EmptyList => LispValue::new_empty_list(),
            Expression::Lambda(x) => LispValue::new_procedure(*x),
            Expression::List{car, cdr} =>
            {
                let car = self.allocate_expression(env, &car.expression);
                let cdr = self.allocate_expression(env, &cdr.expression);

                self.cons(env, car, cdr)
            },
            Expression::Value(x) =>
            {
                let values: Vec<ValueRaw> = x.chars().map(|x| ValueRaw{char: x}).collect();
                let vec = LispVectorRef{
                    tag: ValueTag::Char,
                    values: &values
                };

                let id = self.allocate_vector(env, vec);

                LispValue::new_symbol(id)
            },
            Expression::Application{..} => unreachable!(),
            Expression::Sequence{..} => unreachable!()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Environment<'a>
{
    parent: Option<&'a Environment<'a>>,
    mappings: RefCell<HashMap<String, LispValue>>
}

// this isnt completely safe actually, if theres a Some inside a 'static lifetime env
// then its not safe, but wutever
unsafe impl Send for Environment<'static> {}

impl Environment<'static>
{
    pub fn new() -> Self
    {
        let mappings = RefCell::new(HashMap::new());

        Self{parent: None, mappings}
    }
}

impl<'a> Environment<'a>
{
    pub fn child(parent: &'a Environment<'a>) -> Environment<'a>
    {
        let mappings = RefCell::new(HashMap::new());

        Self{parent: Some(parent), mappings}
    }

    pub fn define(&self, key: impl Into<String>, value: LispValue)
    {
        self.mappings.borrow_mut().insert(key.into(), value);
    }

    pub fn try_lookup(&self, key: &str) -> Option<LispValue>
    {
        self.mappings.borrow().get(key).copied().or_else(||
        {
            self.parent.as_ref().and_then(|parent|
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

pub struct OutputWrapper<'a>
{
    pub memory: &'a mut LispMemory,
    pub value: LispValue
}

impl<'a> Drop for OutputWrapper<'a>
{
    fn drop(&mut self)
    {
        self.memory.clear();
    }
}

impl<'a> OutputWrapper<'a>
{
    pub fn as_vector_ref(&'a self) -> Result<LispVectorRef<'a>, Error>
    {
        self.value.as_vector_ref(self.memory)
    }

    pub fn as_vector(&self) -> Result<LispVector, Error>
    {
        self.value.as_vector(self.memory)
    }

    pub fn as_symbol(&self) -> Result<String, Error>
    {
        self.value.as_symbol(self.memory)
    }

    pub fn as_list(&self) -> Result<LispList, Error>
    {
        self.value.as_list(self.memory)
    }

    pub fn as_integer(self) -> Result<i32, Error>
    {
        self.value.as_integer()
    }
}

pub struct LispConfig
{
    pub environment: Option<Arc<Mutex<Environment<'static>>>>,
    pub lambdas: Option<Lambdas>,
    pub primitives: Arc<Primitives>
}

pub struct Lisp
{
    memory: LispMemory,
    lisp: LispRef
}

impl Lisp
{
    pub fn new(code: &str) -> Result<Self, ErrorPos>
    {
        let memory_size = 1 << 10;
        let memory = LispMemory::new(memory_size);

        Ok(Self{
            memory,
            lisp: LispRef::new(code)?
        })
    }

    pub fn run(&mut self) -> Result<OutputWrapper, ErrorPos>
    {
        self.lisp.run(&mut self.memory)
    }

    pub fn run_environment(&mut self) -> Result<Environment<'static>, ErrorPos>
    {
        self.lisp.run_environment(&mut self.memory)
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

impl Deref for Lisp
{
    type Target = LispRef;

    fn deref(&self) -> &Self::Target
    {
        &self.lisp
    }
}

impl DerefMut for Lisp
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.lisp
    }
}

pub struct LispRef
{
    environment: Option<Arc<Mutex<Environment<'static>>>>,
    program: Program
}

impl LispRef
{
    /// # Safety
    /// if an env has some invalid data it will cause ub
    pub unsafe fn new_with_config(config: LispConfig, code: &str) -> Result<Self, ErrorPos>
    {
        let program = Program::parse(config.primitives, config.lambdas, code)?;

        Ok(Self{program, environment: config.environment})
    }

    pub fn new(code: &str) -> Result<Self, ErrorPos>
    {
        let config = LispConfig{
            environment: None,
            lambdas: None,
            primitives: Arc::new(Primitives::new())
        };

        unsafe{ Self::new_with_config(config, code) }
    }

    pub fn run<'a>(&mut self, memory: &'a mut LispMemory) -> Result<OutputWrapper<'a>, ErrorPos>
    {
        self.run_inner(memory).map(|(_env, value)| value)
    }

    pub fn lambdas(&self) -> &Lambdas
    {
        self.program.lambdas()
    }

    pub fn run_environment(
        &mut self,
        memory: &mut LispMemory
    ) -> Result<Environment<'static>, ErrorPos>
    {
        self.run_inner(memory).map(|(env, _value)| env)
    }

    fn new_environment(&self) -> Environment<'static>
    {
        self.environment.as_ref().map(|x|
        {
            let env: &Environment<'static> = &x.lock();

            Environment::clone(env)
        }).unwrap_or_else(Environment::new)
    }

    fn run_inner<'a>(
        &mut self,
        memory: &'a mut LispMemory
    ) -> Result<(Environment<'static>, OutputWrapper<'a>), ErrorPos>
    {
        let env = self.new_environment();

        self.program.apply(memory, &env)?;
        let value = memory.pop_return();

        let value = OutputWrapper{memory, value};

        Ok((env, value))
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
        let value = output.as_vector().unwrap().as_vec_integer().unwrap();

        assert_eq!(value, vec![1, 2, 3]);
    }

    #[test]
    fn gc()
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

        let memory_size = 64;
        let mut memory = LispMemory::new(memory_size);

        let mut lisp = LispRef::new(code).unwrap();

        let value = lisp.run(&mut memory).unwrap().as_integer().unwrap();

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

        let OutputWrapper{
            ref memory,
            value: ref output
        } = lisp.run().unwrap();

        let value = output.as_list(memory).unwrap();

        list_equals(memory, value, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn list()
    {
        let code = "
            (cons 3 (cons 4 (cons 5 (quote ()))))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let OutputWrapper{
            ref memory,
            value: ref output
        } = lisp.run().unwrap();

        let value = output.as_list(memory).unwrap();

        list_equals(memory, value, &[3, 4, 5]);
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
        let value = output.as_symbol().unwrap();

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
        let value = output.as_vector().unwrap().as_vec_integer().unwrap();

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
        let value = output.as_vector().unwrap().as_vec_integer().unwrap();

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
