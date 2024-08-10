use std::{
    mem,
    rc::Rc,
    cell::RefCell,
    ops::Range,
    fmt::{self, Display, Debug},
    ops::{Deref, DerefMut},
    collections::HashMap
};

use crate::common::write_log;

pub use program::{
    Memoriable,
    MemoryWrapper,
    Program,
    PrimitiveProcedureInfo,
    Primitives,
    Lambdas,
    WithPosition,
    ArgsCount,
    ArgsWrapper
};

use program::{Expression, CodePosition};

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

    pub fn is_null(&self) -> bool
    {
        match self
        {
            Self::EmptyList => true,
            _ => false
        }
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
    pub len: u32,
    pub procedure: u32,
    pub primitive_procedure: u32,
    tag: ValueTag,
    pub special: Special,
    pub list: u32,
    symbol: u32,
    string: u32,
    vector: u32
}

impl PartialEq for ValueRaw
{
    fn eq(&self, other: &Self) -> bool
    {
        unsafe{ self.unsigned == other.unsigned }
    }
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
    PrimitiveProcedure,
    List,
    Vector,
    VectorMoved
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
                | ValueTag::PrimitiveProcedure
                | ValueTag::Symbol
                | ValueTag::VectorMoved
                | ValueTag::Special => false,
            ValueTag::String
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

#[derive(Clone)]
pub struct LispList<T=LispValue>
{
    car: T,
    cdr: T
}

impl<T> LispList<T>
{
    pub fn car(&self) -> &T
    {
        &self.car
    }

    pub fn cdr(&self) -> &T
    {
        &self.cdr
    }

    fn map_ref<U>(&self, mut f: impl FnMut(&T) -> U) -> LispList<U>
    {
        LispList{
            car: f(&self.car),
            cdr: f(&self.cdr)
        }
    }
}

impl From<f32> for LispValue
{
    fn from(x: f32) -> Self
    {
        LispValue::new_float(x)
    }
}

impl From<i32> for LispValue
{
    fn from(x: i32) -> Self
    {
        LispValue::new_integer(x)
    }
}

impl From<bool> for LispValue
{
    fn from(x: bool) -> Self
    {
        LispValue::new_bool(x)
    }
}

impl From<()> for LispValue
{
    fn from(_x: ()) -> Self
    {
        LispValue::new_empty_list()
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
        let s = self.maybe_to_string(None, None);

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

    pub fn new_list(list: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::List, ValueRaw{list})
        }
    }

    pub fn new_vector(vector: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::Vector, ValueRaw{vector})
        }
    }

    pub fn new_string(string: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::String, ValueRaw{string})
        }
    }

    pub fn new_symbol(symbol: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::Symbol, ValueRaw{symbol})
        }
    }

    pub fn new_procedure(procedure: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::Procedure, ValueRaw{procedure})
        }
    }

    pub fn new_primitive_procedure(primitive_procedure: u32) -> Self
    {
        unsafe{
            Self::new(ValueTag::PrimitiveProcedure, ValueRaw{primitive_procedure})
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

    pub fn is_null(&self) -> bool
    {
        self.as_special().map(|x| x.is_null()).unwrap_or(false)
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
            ValueTag::Symbol => Ok(memory.get_symbol(unsafe{ self.value.symbol })),
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

    pub fn as_procedure(self) -> Result<u32, Error>
    {
        match self.tag
        {
            ValueTag::Procedure => Ok(unsafe{ self.value.procedure }),
            x => Err(Error::WrongType{expected: ValueTag::Procedure, got: x})
        }
    }

    pub fn as_primitive_procedure(self) -> Result<u32, Error>
    {
        match self.tag
        {
            ValueTag::PrimitiveProcedure => Ok(unsafe{ self.value.primitive_procedure }),
            x => Err(Error::WrongType{expected: ValueTag::PrimitiveProcedure, got: x})
        }
    }

    pub fn to_string(&self, memory: &LispMemory) -> String
    {
        self.maybe_to_string(
            Some(&memory),
            Some(&memory.memory)
        )
    }

    fn to_string_block(&self, memory: &MemoryBlock) -> String
    {
        self.maybe_to_string(None, Some(memory))
    }

    fn maybe_to_string(
        &self,
        memory: Option<&LispMemory>,
        block: Option<&MemoryBlock>
    ) -> String
    {
        match self.tag
        {
            ValueTag::Integer => unsafe{ self.value.integer.to_string() },
            ValueTag::Float => unsafe{ self.value.float.to_string() },
            ValueTag::Char => unsafe{ self.value.char.to_string() },
            ValueTag::Special => unsafe{ self.value.special.to_string() },
            ValueTag::Procedure => format!("<procedure #{}>", unsafe{ self.value.procedure }),
            ValueTag::PrimitiveProcedure => format!("<primitive procedure #{}>", unsafe{ self.value.primitive_procedure }),
            ValueTag::VectorMoved => "<vector-moved>".to_owned(),
            ValueTag::String => block.map(|memory|
            {
                memory.get_string(unsafe{ self.value.string }).unwrap()
            }).unwrap_or_else(||
            {
                format!("<string {}>", unsafe{ self.value.string })
            }),
            ValueTag::Symbol => memory.map(|memory|
            {
                let s = memory.get_symbol(unsafe{ self.value.symbol });

                format!("'{s}")
            }).unwrap_or_else(||
            {
                format!("<symbol {}>", unsafe{ self.value.symbol })
            }),
            ValueTag::List => block.map(|block|
            {
                let list = block.get_list(unsafe{ self.value.list });

                let car = list.car.maybe_to_string(memory, Some(block));
                let cdr = list.cdr.maybe_to_string(memory, Some(block));

                format!("({car} {cdr})")
            }).unwrap_or_else(||
            {
                format!("<list {}>", unsafe{ self.value.list })
            }),
            ValueTag::Vector => block.map(|block|
            {
                let vec = block.get_vector_ref(unsafe{ self.value.vector });

                let mut s = vec.values.iter().map(|raw| unsafe{ LispValue::new(vec.tag, *raw) })
                    .fold("#(".to_owned(), |acc, value|
                    {
                        let value = value.maybe_to_string(memory, Some(block));

                        acc + &value + " "
                    });

                s.pop();
                s.push(')');

                s
            }).unwrap_or_else(||
            {
                format!("<vector {}>", unsafe{ self.value.vector })
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
    Custom(String),
    NumberParse(String),
    SpecialParse(String),
    UndefinedVariable(String),
    ApplyNonApplication,
    WrongArgumentsCount{proc: String, this_invoked: bool, expected: String, got: usize},
    IndexOutOfRange(i32),
    CharOutOfRange,
    EmptySequence,
    VectorWrongType{expected: ValueTag, got: ValueTag},
    OperationError{a: String, b: String},
    ExpectedNumerical{a: ValueTag, b: ValueTag},
    ExpectedArg,
    ExpectedOp,
    ExpectedClose,
    UnexpectedClose,
    UnexpectedEndOfFile
}

impl Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::WrongType{expected, got} => format!("expected type `{expected:?}` got `{got:?}`"),
            Self::WrongSpecial{expected} => format!("wrong special, expected `{expected:?}`"),
            Self::Custom(s) => s.clone(),
            Self::NumberParse(s) => format!("cant parse `{s}` as number"),
            Self::SpecialParse(s) => format!("cant parse `{s}` as a special"),
            Self::UndefinedVariable(s) => format!("variable `{s}` is undefined"),
            Self::ApplyNonApplication => "apply was called on a non application".to_owned(),
            Self::ExpectedNumerical{a, b} => format!("primitive operation expected 2 numbers, got {a:?} and {b:?}"),
            Self::WrongArgumentsCount{proc, this_invoked: _, expected, got} =>
                format!("wrong amount of arguments (got {got}) passed to {proc} (expected {expected})"),
            Self::IndexOutOfRange(i) => format!("index {i} out of range"),
            Self::CharOutOfRange => "char out of range".to_owned(),
            Self::EmptySequence => "empty sequence".to_owned(),
            Self::VectorWrongType{expected, got} =>
                format!("vector expected `{expected:?}` got `{got:?}`"),
            Self::OperationError{a, b} =>
                format!("numeric error with {a} and {b} operands"),
            Self::ExpectedArg => "expected an argument".to_owned(),
            Self::ExpectedOp => "expected an operator".to_owned(),
            Self::ExpectedClose => "expected a closing parenthesis".to_owned(),
            Self::UnexpectedClose => "unexpected closing parenthesis".to_owned(),
            Self::UnexpectedEndOfFile => "unexpected end of file".to_owned()
        };

        write!(f, "{}", s)
    }
}

fn clone_with_capacity<T: Clone>(v: &Vec<T>) -> Vec<T>
{
    let mut new_v = Vec::with_capacity(v.capacity());

    new_v.extend(v.iter().cloned());

    new_v
}

struct MemoryBlock
{
    general: Vec<ValueRaw>,
    cars: Vec<LispValue>,
    cdrs: Vec<LispValue>
}

impl Clone for MemoryBlock
{
    fn clone(&self) -> Self
    {
        Self{
            general: clone_with_capacity(&self.general),
            cars: clone_with_capacity(&self.cars),
            cdrs: clone_with_capacity(&self.cdrs)
        }
    }
}

impl Debug for MemoryBlock
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        // all fields r 32 bits long and u32 has no invalid states
        let general = self.general.iter().map(|raw| unsafe{ raw.unsigned }).collect::<Vec<u32>>();

        let pv = |v: &LispValue|
        {
            v.to_string_block(self)
        };

        let cars = self.cars.iter().map(pv).collect::<Vec<_>>();
        let cdrs = self.cdrs.iter().map(pv).collect::<Vec<_>>();

        f.debug_struct("MemoryBlock")
            .field("cars", &cars)
            .field("cdrs", &cdrs)
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

    fn vector_raw_info(&self, id: u32) -> (ValueTag, usize)
    {
        let id = id as usize;

        let len = unsafe{ self.general[id].len };
        let tag = unsafe{ self.general[id + 1].tag };

        (tag, len as usize)
    }

    fn vector_info(&self, id: u32) -> (ValueTag, Range<usize>)
    {
        let (tag, len) = self.vector_raw_info(id);

        let start = (id + 2) as usize;
        (tag, start..(start + len))
    }

    pub fn get_vector_ref(&self, id: u32) -> LispVectorRef
    {
        let (tag, range) = self.vector_info(id);

        LispVectorRef{
            tag,
            values: &self.general[range]
        }
    }

    pub fn get_vector_mut(&mut self, id: u32) -> LispVectorMut
    {
        let (tag, range) = self.vector_info(id);

        LispVectorMut{
            tag,
            values: &mut self.general[range]
        }
    }

    pub fn get_vector(&self, id: u32) -> LispVector
    {
        let (tag, range) = self.vector_info(id);

        LispVector{
            tag,
            values: self.general[range].to_vec()
        }
    }

    pub fn get_string(&self, id: u32) -> Result<String, Error>
    {
        let vec = self.get_vector_ref(id);

        if vec.tag != ValueTag::Char
        {
            return Err(Error::VectorWrongType{expected: ValueTag::Char, got: vec.tag});
        }

        Ok(vec.values.iter().map(|x| unsafe{ x.char }).collect())
    }

    pub fn get_list(&self, id: u32) -> LispList
    {
        LispList{
            car: self.get_car(id),
            cdr: self.get_cdr(id)
        }
    }

    pub fn get_car(&self, id: u32) -> LispValue
    {
        self.cars[id as usize]
    }

    pub fn get_cdr(&self, id: u32) -> LispValue
    {
        self.cdrs[id as usize]
    }

    pub fn set_car(&mut self, id: u32, value: LispValue)
    {
        self.cars[id as usize] = value;
    }

    pub fn set_cdr(&mut self, id: u32, value: LispValue)
    {
        self.cdrs[id as usize] = value;
    }

    pub fn cons(&mut self, car: LispValue, cdr: LispValue) -> LispValue
    {
        let id = self.cars.len();

        debug_assert!(self.cdrs.len() == id);

        self.cars.push(car);
        self.cdrs.push(cdr);

        LispValue::new_list(id as u32)
    }

    fn allocate_iter<'a>(
        &mut self,
        tag: ValueTag,
        iter: impl ExactSizeIterator<Item=&'a ValueRaw>
    ) -> u32
    {
        let id = self.general.len() as u32;

        let len = iter.len() as u32;
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

#[derive(Clone, Debug)]
pub struct LispMemory
{
    stack_size: usize,
    symbols: Vec<String>,
    lambdas: Lambdas,
    memory: MemoryBlock,
    swap_memory: MemoryBlock,
    returns: Vec<LispValue>
}

impl Default for LispMemory
{
    fn default() -> Self
    {
        Self::new(256, 1 << 10)
    }
}

impl LispMemory
{
    pub fn new(stack_size: usize, memory_size: usize) -> Self
    {
        let symbols = Vec::new();
        let memory = MemoryBlock::new(memory_size);
        let swap_memory = MemoryBlock::new(memory_size);

        Self{
            stack_size,
            symbols,
            lambdas: Lambdas::new(),
            memory,
            swap_memory,
            returns: Vec::with_capacity(stack_size)
        }
    }

    pub fn lambdas(&self) -> &Lambdas
    {
        &self.lambdas
    }

    pub fn lambdas_mut(&mut self) -> &mut Lambdas
    {
        &mut self.lambdas
    }

    pub fn no_memory() -> Self
    {
        Self::new(256, 0)
    }

    pub fn clear(&mut self)
    {
        self.memory.clear();
        self.lambdas.clear();
    }

    fn transfer_to_swap_value(
        memory: &mut MemoryBlock,
        swap_memory: &mut MemoryBlock,
        value: LispValue
    ) -> LispValue
    {
        match value.tag
        {
            ValueTag::List =>
            {
                let id = unsafe{ value.value.list };

                let list = memory.get_list(id);

                if list.car.is_broken_heart()
                {
                    return list.cdr;
                }

                let new_value = swap_memory.cons(list.car, list.cdr);

                memory.set_car(id, LispValue::new_broken_heart());
                memory.set_cdr(id, new_value);

                new_value
            },
            ValueTag::Vector | ValueTag::String =>
            {
                let id = match value.tag
                {
                    ValueTag::Vector => unsafe{ value.value.vector },
                    ValueTag::String => unsafe{ value.value.string },
                    _ => unreachable!()
                } as usize;

                let create_id = |id: u32|
                {
                    match value.tag
                    {
                        ValueTag::String => LispValue::new_string(id),
                        ValueTag::Vector => LispValue::new_vector(id),
                        _ => unreachable!()
                    }
                };

                if let ValueTag::VectorMoved = unsafe{ memory.general[id + 1].tag }
                {
                    return create_id(unsafe{ memory.general[id].vector });
                }

                let (tag, range) = memory.vector_info(id as u32);

                let new_id = swap_memory.allocate_iter(tag, memory.general[range].iter());

                memory.general[id] = ValueRaw{vector: new_id};
                memory.general[id + 1] = ValueRaw{tag: ValueTag::VectorMoved};

                create_id(new_id as u32)
            },
            _ => value
        }
    }

    fn transfer_env(&mut self, env: &Environment)
    {
        if let Environment::Child(parent, _) = env
        {
            self.transfer_env(parent);
        }

        env.mappings().0.borrow_mut().iter_mut().map(|(_key, value)| value).for_each(|value|
        {
            *value = Self::transfer_to_swap_value(&mut self.memory, &mut self.swap_memory, *value);
        });
    }

    fn transfer_returns(&mut self)
    {
        self.returns.iter_mut().for_each(|value|
        {
            *value = Self::transfer_to_swap_value(&mut self.memory, &mut self.swap_memory, *value);
        });
    }

    pub fn gc(&mut self, env: &Environment)
    {
        self.transfer_env(env);
        self.transfer_returns();

        macro_rules! transfer_pair
        {
            ($part:ident, $scan:expr) =>
            {
                while $scan < self.swap_memory.$part.len()
                {
                    let value = self.swap_memory.$part[$scan];
                    self.swap_memory.$part[$scan] = Self::transfer_to_swap_value(
                        &mut self.memory,
                        &mut self.swap_memory,
                        value
                    );

                    $scan += 1;
                }
            }
        }

        let mut cars_scan = 0;
        let mut cdrs_scan = 0;

        let mut general_tag = ValueTag::Integer;
        let mut general_scan = 0;
        let mut general_remaining = 0;

        while cars_scan < self.swap_memory.cars.len()
            || cdrs_scan < self.swap_memory.cdrs.len()
            || general_scan < self.swap_memory.general.len()
        {
            transfer_pair!(cars, cars_scan);
            transfer_pair!(cdrs, cdrs_scan);

            while general_scan < self.swap_memory.general.len()
            {
                if general_remaining == 0
                {
                    (general_tag, general_remaining) = self.swap_memory
                        .vector_raw_info(general_scan as u32);

                    general_scan += 2;
                }

                if general_remaining > 0
                {
                    if general_tag.is_boxed()
                    {
                        let value = self.swap_memory.general[general_scan];
                        self.swap_memory.general[general_scan] = Self::transfer_to_swap_value(
                            &mut self.memory,
                            &mut self.swap_memory,
                            LispValue{tag: general_tag, value}
                        ).value;

                        general_remaining -= 1;
                        general_scan += 1;
                    } else
                    {
                        general_scan += general_remaining;
                        general_remaining = 0;
                    }
                }
            }
        }

        debug_assert!(general_remaining == 0);

        mem::swap(&mut self.memory, &mut self.swap_memory);

        self.swap_memory.clear();
    }

    pub fn push_return(&mut self, value: impl Into<LispValue>)
    {
        if self.returns.len() >= self.stack_size
        {
            panic!("stack overflow!!!! ahhhh!!");
        }

        self.returns.push(value.into());
    }

    pub fn pop_return(&mut self) -> LispValue
    {
        self.try_pop_return().expect("cant pop from an empty stack, how did this happen?")
    }

    pub fn try_pop_return(&mut self) -> Option<LispValue>
    {
        self.returns.pop()
    }

    pub fn returns_len(&self) -> usize
    {
        self.returns.len()
    }

    pub fn get_vector_ref(&self, id: u32) -> LispVectorRef
    {
        self.memory.get_vector_ref(id)
    }

    pub fn get_vector_mut(&mut self, id: u32) -> LispVectorMut
    {
        self.memory.get_vector_mut(id)
    }

    pub fn get_vector(&self, id: u32) -> LispVector
    {
        self.memory.get_vector(id)
    }

    pub fn get_symbol(&self, id: u32) -> String
    {
        self.symbols[id as usize].clone()
    }

    pub fn get_string(&self, id: u32) -> Result<String, Error>
    {
        self.memory.get_string(id)
    }

    pub fn get_list(&self, id: u32) -> LispList
    {
        self.memory.get_list(id)
    }

    #[allow(dead_code)]
    pub fn get_car(&self, id: u32) -> LispValue
    {
        self.memory.get_car(id)
    }

    #[allow(dead_code)]
    pub fn get_cdr(&self, id: u32) -> LispValue
    {
        self.memory.get_car(id)
    }

    fn out_of_memory(&mut self) -> !
    {
        write_log(format!("{:#?}\n", &self.memory));
        panic!("out of memory, memory dump saved to log");
    }

    fn need_list_memory(&mut self, env: &Environment, amount: usize)
    {
        if self.memory.list_remaining() < amount
        {
            self.gc(env);

            if self.memory.list_remaining() < amount
            {
                self.out_of_memory()
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
                self.out_of_memory()
            }
        }
    }

    pub fn cons(
        &mut self,
        env: &Environment
    )
    {
        self.cons_inner(env, |this|
        {
            let cdr = this.pop_return();
            let car = this.pop_return();

            (car, cdr)
        })
    }

    pub fn rcons(
        &mut self,
        env: &Environment
    )
    {
        self.cons_inner(env, |this|
        {
            let car = this.pop_return();
            let cdr = this.pop_return();

            (car, cdr)
        })
    }

    fn cons_inner(
        &mut self,
        env: &Environment,
        f: impl FnOnce(&mut Self) -> (LispValue, LispValue)
    )
    {
        self.need_list_memory(env, 1);

        let (car, cdr) = f(self);

        let pair = self.memory.cons(car, cdr);

        self.push_return(pair);
    }

    pub fn cons_list<I, V>(
        &mut self,
        env: &Environment,
        values: I
    )
    where
        V: Into<LispValue>,
        I: IntoIterator<Item=V>,
        I::IntoIter: DoubleEndedIterator + ExactSizeIterator
    {
        let iter = values.into_iter();
        let len = iter.len();

        iter.for_each(|x| self.push_return(x));

        self.push_return(());

        (0..len).for_each(|_| self.cons(env));
    }

    pub fn new_symbol(
        &mut self,
        x: impl Into<String>
    ) -> LispValue
    {
        let x = x.into();

        let id = self.symbols.iter().position(|symbol| symbol == &x).unwrap_or_else(||
        {
            let id = self.symbols.len();
            self.symbols.push(x);

            id
        });

        LispValue::new_symbol(id as u32)
    }

    pub fn allocate_vector(
        &mut self,
        env: &Environment,
        vec: LispVectorInner<&[ValueRaw]>
    )
    {
        let len = vec.values.len();

        // +2 for the length and for the type tag
        self.need_memory(env, len + 2);

        let id = self.memory.allocate_iter(vec.tag, vec.values.iter());

        self.push_return(LispValue::new_vector(id));
    }

    pub fn allocate_expression(
        &mut self,
        env: &Environment,
        expression: &Expression
    )
    {
        let value = match expression
        {
            Expression::Float(x) => LispValue::new_float(*x),
            Expression::Integer(x) => LispValue::new_integer(*x),
            Expression::Bool(x) => LispValue::new_bool(*x),
            Expression::EmptyList => LispValue::new_empty_list(),
            Expression::Value(x) => self.new_symbol(x),
            Expression::List{car, cdr} =>
            {
                self.allocate_expression(env, &car.expression);
                self.allocate_expression(env, &cdr.expression);

                self.cons(env);

                return;
            },
            Expression::Lambda{..} => unreachable!(),
            Expression::Application{..} => unreachable!(),
            Expression::Sequence{..} => unreachable!()
        };

        self.push_return(value);
    }
}

#[derive(Debug, Clone)]
pub struct Mappings(pub RefCell<HashMap<String, LispValue>>);

impl Mappings
{
    pub fn new() -> Self
    {
        Self(RefCell::new(HashMap::new()))
    }

    pub fn define(&self, key: impl Into<String>, value: LispValue)
    {
        self.0.borrow_mut().insert(key.into(), value);
    }

    pub fn try_lookup(&self, key: &str) -> Option<LispValue>
    {
        self.0.borrow().get(key).copied()
    }
}

unsafe impl Send for Mappings {}

#[derive(Debug, Clone)]
pub enum Environment
{
    TopLevel(Mappings),
    Child(Rc<Environment>, Mappings)
}

impl Environment
{
    pub fn new() -> Self
    {
        Self::TopLevel(Mappings::new())
    }

    pub fn top_level(mappings: Mappings) -> Self
    {
        Self::TopLevel(mappings)
    }

    pub fn child(parent: Rc<Environment>) -> Self
    {
        Self::Child(parent, Mappings::new())
    }

    pub fn mappings(&self) -> &Mappings
    {
        match self
        {
            Self::TopLevel(x) => x,
            Self::Child(_, x) => x
        }
    }

    pub fn define(&self, key: impl Into<String>, value: LispValue)
    {
        self.mappings().define(key, value);
    }

    pub fn try_lookup(&self, key: &str) -> Option<LispValue>
    {
        let this_lookup = self.mappings().try_lookup(key);

        match self
        {
            Self::TopLevel(_) => this_lookup,
            Self::Child(parent, _) => 
            {
                this_lookup.or_else(||
                {
                    parent.try_lookup(key)
                })
            }
        }
    }

    pub fn lookup(&self, key: &str) -> Result<LispValue, Error>
    {
        self.try_lookup(key).ok_or_else(|| Error::UndefinedVariable(key.to_owned()))
    }
}

#[derive(Clone)]
pub struct OutputWrapper<'a>
{
    pub memory: &'a LispMemory,
    pub value: LispValue
}

impl<'a> Display for OutputWrapper<'a>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.value.to_string(self.memory))
    }
}

impl<'a> Deref for OutputWrapper<'a>
{
    type Target = LispValue;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<'a> OutputWrapper<'a>
{
    pub fn into_value(self) -> LispValue
    {
        self.value
    }

    pub fn as_vector_ref(&self) -> Result<LispVectorRef<'a>, Error>
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

    pub fn as_list(&self) -> Result<LispList<OutputWrapper<'a>>, Error>
    {
        let lst = self.value.as_list(self.memory)?;

        Ok(lst.map_ref(|value|
        {
            OutputWrapper{memory: self.memory, value: *value}
        }))
    }

    pub fn as_float(&self) -> Result<f32, Error>
    {
        self.value.as_float()
    }

    pub fn as_integer(&self) -> Result<i32, Error>
    {
        self.value.as_integer()
    }

    pub fn as_bool(&self) -> Result<bool, Error>
    {
        self.value.as_bool()
    }
}

pub struct LispConfig
{
    pub environment: Option<Rc<Environment>>,
    pub primitives: Rc<Primitives>
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
        let memory = Self::default_memory();

        Ok(Self{
            memory,
            lisp: LispRef::new(code)?
        })
    }

    pub fn new_env(code: &str) -> Result<Rc<Environment>, ErrorPos>
    {
        let mut lisp = Lisp::new(code)?;

        Ok(lisp.run_environment()?)
    }

    pub fn empty_memory() -> LispMemory
    {
        LispMemory::no_memory()
    }

    pub fn default_memory() -> LispMemory
    {
        LispMemory::default()
    }

    pub fn run(&mut self) -> Result<OutputWrapper, ErrorPos>
    {
        self.lisp.run_with_memory(&mut self.memory)
    }

    pub fn run_environment(&mut self) -> Result<Rc<Environment>, ErrorPos>
    {
        let result = self.lisp.run_environment(&mut self.memory);

        debug_assert!(self.memory.returns_len() == 0);

        result
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

#[derive(Debug)]
pub struct LispRef
{
    program: Program
}

impl LispRef
{
    /// # Safety
    /// if an env has some invalid data it will cause ub
    pub unsafe fn new_with_config(config: LispConfig, code: &str) -> Result<Self, ErrorPos>
    {
        let environment = config.environment.unwrap_or_else(||
        {
            Rc::new(Environment::new())
        });

        let program = Program::parse(
            environment,
            config.primitives,
            code
        )?;

        Ok(Self{program})
    }

    pub fn new(code: &str) -> Result<Self, ErrorPos>
    {
        let config = LispConfig{
            environment: None,
            primitives: Rc::new(Primitives::new())
        };

        unsafe{ Self::new_with_config(config, code) }
    }

    pub fn run_with_memory<'a>(
        &mut self,
        memory: &'a mut LispMemory
    ) -> Result<OutputWrapper<'a>, ErrorPos>
    {
        self.run_with_memory_environment(memory).map(|(_env, value)| value)
    }

    pub fn run_env(
        &mut self,
        memory: &mut LispMemory
    ) -> Result<Rc<Environment>, ErrorPos>
    {
        let env = self.run_environment(memory)?;

        Ok(env)
    }

    pub fn run_environment(
        &mut self,
        memory: &mut LispMemory
    ) -> Result<Rc<Environment>, ErrorPos>
    {
        self.run_with_memory_environment(memory).map(|(env, _value)| env)
    }

    pub fn run_with_memory_environment<'a>(
        &mut self,
        memory: &'a mut LispMemory
    ) -> Result<(Rc<Environment>, OutputWrapper<'a>), ErrorPos>
    {
        let (env, value) = self.program.eval(memory)?;

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
    fn define_lambdas()
    {
        let code = "
            (define one (lambda (x) (+ x 5)))
            (define two one)

            (+ (two 4) (one 5))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 19);
    }

    #[test]
    fn define_many()
    {
        let code = "
            (define v (make-vector 2 0))
            (define (thingy x)
                (vector-set! v 0 x)
                x)

            (define x (thingy 4))
            (+ x (vector-ref v 0))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 8);
    }

    #[test]
    fn redefine_primitive()
    {
        let code = "
            (define cooler-cons cons)

            (define l (cooler-cons 5 3))

            (+ (car l) (cdr l))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 8);
    }

    #[test]
    fn derivative()
    {
        let code = "
            (define (derivative f)
                (define epsilon 0.0001)
                (lambda (x)
                    (let ((low (f (- x epsilon))) (high (f (+ x epsilon))))
                        (/ (- high low) (+ epsilon epsilon)))))

            (define (square x) (* x x))

            ((derivative square) 0.5)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run()
            .unwrap()
            .as_float()
            .unwrap();

        assert!(value > 0.9999 && value <= 1.0, "{value}");
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
    fn gc_simple()
    {
        let code = "
            (define old-list (cons (cons 1 2) (cons 3 4)))

            (define (number x)
                (display
                    (car
                        (cons
                            x
                            (cons (+ x 1) (+ x 100))))))

            (number 5)
            (number 7)
            (number 9)
            (number 11)
            (number 13)
            (number 15)
            (number 17)
            (number 19)

            (car (cdr old-list))
        ";

        let memory_size = 24;
        let mut memory = LispMemory::new(5, memory_size);

        let mut lisp = LispRef::new(code).unwrap();

        let value = lisp.run_with_memory(&mut memory)
            .unwrap()
            .as_integer()
            .unwrap();

        assert_eq!(value, 3_i32);
    }

    #[test]
    fn gc_list()
    {
        let code = "
            (define old-list (cons (cons 1 2) (cons 3 4)))

            (display old-list)

            (define (factorial-list n)
                (if (= n 1)
                    (quote (1))
                    (let ((next (factorial-list (- n 1))))
                        (let ((this (* n (car next))))
                            (cons this next)))))

            (define (silly x) (car (factorial-list x)))

            (display old-list)
            (define a (+ (silly 7) (silly 5) (silly 6) (silly 11) (silly 4)))
            (display old-list)

            (display a)

            (define b (car (cdr old-list)))

            (display b)

            (+ a b)
        ";

        let memory_size = 64;
        let mut memory = LispMemory::new(32, memory_size);

        let mut lisp = LispRef::new(code).unwrap();

        let value = lisp.run_with_memory(&mut memory)
            .unwrap()
            .as_integer()
            .unwrap();

        assert_eq!(value, 39_922_707_i32);
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
    fn quoting_tick_list()
    {
        let code = "
            '(1 2 3 4 5)
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
    fn quoting_tick()
    {
        let code = "
            'heyyy
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let OutputWrapper{
            ref memory,
            value: ref output
        } = lisp.run().unwrap();

        let value = output.as_symbol(memory).unwrap();

        assert_eq!(value, "heyyy".to_owned());
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
    fn gc_vector()
    {
        let amount = 100;
        let code = format!("
            (define x (make-vector 5 999))

            (define make-pair cons)
            (define pair-x car)
            (define pair-y cdr)

            ; more opportunities for gc bugs this way!
            (define (pair-index p) (+ (pair-x p) (pair-y p)))

            (vector-set! x 3 123)
            (vector-set! x 2 5)
            (vector-set! x 1 9)
            (vector-set! x 4 1000)

            (define (inc-by-1! x p)
                (vector-set! x (pair-index p) (+ (vector-ref x (pair-index p)) 1)))

            (define (loop f i)
                (if (= i 0)
                    '()
                    (begin
                        (f (- i 1))
                        (loop f (- i 1)))))

            (loop
                (lambda (i) (vector-set! x i 0))
                5)

            (loop
                (lambda (j)
                    (begin
                        (display '(lots of stuff and allocations of lists so it triggers a gc))
                        (loop
                            (lambda (i) (inc-by-1! x (make-pair 3 (- i 3))))
                            5)))
                {amount}) ; this should be 1000 but rust doesnt guarantee tail call optimization
            ; so it overflows the stack

            (inc-by-1! x (make-pair 3 1))

            x
        ");

        let memory_size = 50;
        let mut memory = LispMemory::new(256, memory_size);

        let mut lisp = LispRef::new(&code).unwrap();

        let output = lisp.run_with_memory(&mut memory).unwrap();

        let value = output.as_vector().unwrap().as_vec_integer().unwrap();

        assert_eq!(value, vec![amount, amount, amount, amount, amount + 1]);
    }

    #[test]
    fn variadic_lambda()
    {
        let code = "
            (define (fold f start xs)
                (if (null? xs)
                    start
                    (fold f (f (car xs) start) (cdr xs))))
                    
            (define f (lambda xs
                (fold + 0 xs)))

            (+ (f 1 2 3) (f 5) (f))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 11_i32);
    }

    #[test]
    fn comments()
    {
        let code = "
            ; hey this is a comment
            ;this too is a comment
            (+ 1 2 3) ; this adds some numbers!
            ; yea
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 6);
    }

    #[test]
    fn displaying_one()
    {
        let code = "
            (display 'hey)

            0
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 0);
    }

    #[test]
    fn displaying()
    {
        let code = "
            (display 'hey)
            (display 'nice)
            (display '(very nested stuff over here woooooo nice pro cool cooler cooleo #t cool true 3))

            0
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 0);
    }

    #[test]
    fn displaying_lots()
    {
        let code = "
            (define (print-garbage)
                (display 'hey)
                (display 'nice)
                (display '(very nested stuff over here woooooo nice pro cool cooler cooleo #t cool true 3)))

            (define (loop f i)
                (if (= i 0)
                    '()
                    (begin
                        (f)
                        (loop f (- i 1)))))
            
            (loop print-garbage 50)

            0
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 0);
    }

    #[test]
    fn booleans()
    {
        let code = "
            (if #t 2 3)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 2_i32);
    }

    #[test]
    fn random()
    {
        let code = "
            (random-integer 10)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert!((0..10).contains(&value));
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
    fn if_no_else()
    {
        let code = "
            (if (null? (if (= 5 1) 8))
                1)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_integer().unwrap();

        assert_eq!(value, 1_i32);
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
