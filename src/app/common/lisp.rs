use std::{
    mem,
    borrow::Borrow,
    rc::Rc,
    ops::Range,
    fmt::{self, Display, Debug},
    ops::Deref,
    collections::HashMap
};

use strum::EnumCount;

pub use program::{
    Program,
    PrimitiveProcedureInfo,
    Primitives,
    WithPosition,
    ArgsCount
};

use program::{PrimitiveType, Register, CodePosition};

mod program;


#[derive(Clone, Copy)]
pub union ValueRaw
{
    unsigned: u32,
    address: u32,
    pub integer: i32,
    pub float: f32,
    pub character: char,
    pub len: u32,
    pub procedure: u32,
    pub primitive_procedure: u32,
    tag: ValueTag,
    pub boolean: bool,
    pub list: u32,
    symbol: SymbolId,
    string: u32,
    vector: u32,
    empty: ()
}

impl PartialEq for ValueRaw
{
    fn eq(&self, other: &Self) -> bool
    {
        unsafe{ self.unsigned == other.unsigned }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueTag
{
    Integer,
    Float,
    Char,
    String,
    Symbol,
    Bool,
    EmptyList,
    PrimitiveProcedure,
    List,
    Vector,
    Address,
    EnvironmentMarker,
    BrokenHeart,
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
                | ValueTag::Symbol
                | ValueTag::Bool
                | ValueTag::EmptyList
                | ValueTag::PrimitiveProcedure
                | ValueTag::Address
                | ValueTag::EnvironmentMarker
                | ValueTag::BrokenHeart
                | ValueTag::VectorMoved => false,
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

impl LispVector
{
    pub fn as_ref_vector(&self) -> LispVectorInner<&[ValueRaw]>
    {
        LispVectorInner{
            tag: self.tag,
            values: &self.values
        }
    }
}

impl<T: Deref<Target=[ValueRaw]>> LispVectorInner<T>
{
    /// # Safety
    /// all vectors must be valid therefore the tag and values r in sync
    pub fn try_get(&self, index: usize) -> Option<LispValue>
    {
        self.values.get(index).map(|value| unsafe{ LispValue::new(self.tag, *value) })
    }

    pub fn get(&self, index: usize) -> LispValue
    {
        self.try_get(index).unwrap_or_else(||
        {
            panic!("index {index} out of bounds")
        })
    }

    pub fn len(&self) -> usize
    {
        self.values.len()
    }
}

impl<T: LispValuable> From<Vec<T>> for LispVector
{
    fn from(v: Vec<T>) -> Self
    {
        Self{
            tag: <T as LispValuable>::tag(),
            values: v.into_iter().map(|x|
            {
                let value: LispValue = x.into();

                value.value
            }).collect()
        }
    }
}

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

#[derive(Debug, Clone)]
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

pub trait LispValuable: Into<LispValue>
{
    fn tag() -> ValueTag;
}

impl From<f32> for LispValue
{
    fn from(x: f32) -> Self
    {
        LispValue::new_float(x)
    }
}

impl LispValuable for f32
{
    fn tag() -> ValueTag { ValueTag::Float }
}

impl From<i32> for LispValue
{
    fn from(x: i32) -> Self
    {
        LispValue::new_integer(x)
    }
}

impl LispValuable for i32
{
    fn tag() -> ValueTag { ValueTag::Integer }
}

impl From<bool> for LispValue
{
    fn from(x: bool) -> Self
    {
        LispValue::new_bool(x)
    }
}

impl LispValuable for bool
{
    fn tag() -> ValueTag { ValueTag::Bool }
}

impl From<()> for LispValue
{
    fn from(_x: ()) -> Self
    {
        LispValue::new_empty_list()
    }
}

impl LispValuable for ()
{
    fn tag() -> ValueTag { ValueTag::EmptyList }
}

#[derive(Clone, Copy, PartialEq)]
pub struct LispValue
{
    tag: ValueTag,
    value: ValueRaw
}

impl Debug for LispValue
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let mut visited = Vec::new();
        let s = self.maybe_to_string(&mut visited, None, None);

        write!(f, "{s}")
    }
}

macro_rules! implement_tagged
{
    ($(($new_func:ident, $as_func:ident, $value_type:ident, $union_name:ident, $tag_name:ident)),+) =>
    {
        $(
            pub fn $new_func($union_name: $value_type) -> Self
            {
                unsafe{
                    Self::new(ValueTag::$tag_name, ValueRaw{$union_name})
                }
            }

            pub fn $as_func(self) -> Result<$value_type, Error>
            {
                match self.tag
                {
                    ValueTag::$tag_name => Ok(unsafe{ self.value.$union_name }),
                    x => Err(Error::WrongType{expected: ValueTag::$tag_name, got: x})
                }
            }
        )+
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

    implement_tagged!{
        (new_integer, as_integer, i32, integer, Integer),
        (new_float, as_float, f32, float, Float),
        (new_char, as_char, char, character, Char),
        (new_bool, as_bool, bool, boolean, Bool),
        (new_address, as_address, u32, address, Address),
        (new_primitive_procedure, as_primitive_procedure, u32, primitive_procedure, PrimitiveProcedure),
        (new_symbol_id, as_symbol_id, SymbolId, symbol, Symbol),
        (new_list_id, as_list_id, u32, list, List)
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

    pub fn new_empty_list() -> Self
    {
        unsafe{
            Self::new(ValueTag::EmptyList, ValueRaw{empty: ()})
        }
    }

    pub fn new_environment_marker() -> Self
    {
        unsafe{
            Self::new(ValueTag::EnvironmentMarker, ValueRaw{empty: ()})
        }
    }

    pub fn new_broken_heart() -> Self
    {
        unsafe{
            Self::new(ValueTag::BrokenHeart, ValueRaw{empty: ()})
        }
    }

    pub fn tag(&self) -> ValueTag
    {
        self.tag
    }

    pub fn is_null(&self) -> bool
    {
        self.tag == ValueTag::EmptyList
    }

    pub fn is_broken_heart(&self) -> bool
    {
        self.tag == ValueTag::BrokenHeart
    }

    pub fn is_true(&self) -> bool
    {
        if self.tag == ValueTag::Bool { unsafe{ self.value.boolean } } else { true }
    }

    pub fn as_symbol(self, memory: &LispMemory) -> Result<String, Error>
    {
        self.as_symbol_id().map(|id| memory.get_symbol(id))
    }

    pub fn as_list(self, memory: &LispMemory) -> Result<LispList, Error>
    {
        self.as_list_id().map(|id| memory.get_list(id))
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

    pub fn to_string(&self, memory: &LispMemory) -> String
    {
        let mut visited = Vec::new();
        self.maybe_to_string(
            &mut visited,
            Some(memory),
            Some(&memory.memory)
        )
    }

    // cope, clippy
    #[allow(clippy::wrong_self_convention)]
    #[allow(dead_code)]
    fn to_string_block(&self, memory: &MemoryBlock) -> String
    {
        let mut visited = Vec::new();
        self.maybe_to_string(&mut visited, None, Some(memory))
    }

    fn maybe_to_string(
        &self,
        visited_boxed: &mut Vec<LispValue>,
        memory: Option<&LispMemory>,
        block: Option<&MemoryBlock>
    ) -> String
    {
        if self.tag.is_boxed()
        {
            let contains = visited_boxed.contains(self);

            if contains
            {
                return "<cycle detected>".to_owned();
            } else
            {
                visited_boxed.push(*self);
            }
        }

        match self.tag
        {
            ValueTag::Integer => unsafe{ self.value.integer.to_string() },
            ValueTag::Float => unsafe{ self.value.float.to_string() },
            ValueTag::Char => unsafe{ self.value.character.to_string() },
            ValueTag::PrimitiveProcedure => format!("<primitive procedure #{}>", unsafe{ self.value.primitive_procedure }),
            ValueTag::Bool => if unsafe{ self.value.boolean } { "#t" } else { "#f" }.to_owned(),
            ValueTag::EmptyList => "()".to_owned(),
            ValueTag::EnvironmentMarker => "<environment>".to_owned(),
            ValueTag::VectorMoved => "<vector-moved>".to_owned(),
            ValueTag::BrokenHeart => "<broken-heart>".to_owned(),
            ValueTag::Address => format!("<address {}>", unsafe{ self.value.address.to_string() }),
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

                let car = list.car.maybe_to_string(visited_boxed, memory, Some(block));
                let cdr = list.cdr.maybe_to_string(visited_boxed, memory, Some(block));

                format!("({car} {cdr})")
            }).unwrap_or_else(||
            {
                format!("<list {}>", unsafe{ self.value.list })
            }),
            ValueTag::Vector => block.map(|block|
            {
                let vec = block.get_vector_ref(unsafe{ self.value.vector });

                let mut leftover_space = false;
                let mut s = vec.values.iter().map(|raw| unsafe{ LispValue::new(vec.tag, *raw) })
                    .fold("#(".to_owned(), |acc, value|
                    {
                        leftover_space = true;
                        let value = value.maybe_to_string(visited_boxed, memory, Some(block));

                        acc + &value + " "
                    });

                if leftover_space
                {
                    s.pop();
                }

                s.push(')');

                s
            }).unwrap_or_else(||
            {
                format!("<vector {}>", unsafe{ self.value.vector })
            })
        }
    }
}

pub type ErrorPos = WithPosition<Error>;

impl Display for ErrorPos
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}: {}", self.position, self.value)
    }
}

#[derive(Debug, Clone)]
pub enum Error
{
    OutOfMemory,
    WrongType{expected: ValueTag, got: ValueTag},
    WrongConditionalType(String),
    Custom(String),
    NumberParse(String),
    SpecialParse(String),
    CharTooLong(String),
    UndefinedVariable(String),
    DefineEmptyList,
    AttemptedShadowing(String),
    CallNonProcedure{got: String},
    WrongArgumentsCount{proc: String, this_invoked: bool, expected: String, got: Option<usize>},
    IndexOutOfRange(i32),
    CharOutOfRange,
    EmptySequence,
    VectorWrongType{expected: ValueTag, got: ValueTag},
    OperationError{a: String, b: String},
    ExpectedNumerical{a: ValueTag, b: ValueTag},
    ExpectedList,
    ExpectedParam,
    ExpectedOp,
    ExpectedClose,
    ExpectedSymbol,
    UnexpectedClose,
    UnexpectedEndOfFile
}

impl Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::OutOfMemory => "out of memory".to_owned(),
            Self::WrongType{expected, got} => format!("expected type `{expected:?}` got `{got:?}`"),
            Self::WrongConditionalType(s) => format!("conditional expected boolean, got `{s}`"),
            Self::Custom(s) => s.clone(),
            Self::NumberParse(s) => format!("cant parse `{s}` as number"),
            Self::SpecialParse(s) => format!("cant parse `{s}` as a special"),
            Self::CharTooLong(s) => format!("cant parse `{s}` as char"),
            Self::UndefinedVariable(s) => format!("variable `{s}` is undefined"),
            Self::DefineEmptyList => "cannot define an empty list".to_owned(),
            Self::AttemptedShadowing(s) => format!("attempted to shadow `{s}` which is a primitive"),
            Self::ExpectedNumerical{a, b} => format!("primitive operation expected 2 numbers, got {a:?} and {b:?}"),
            Self::CallNonProcedure{got} => format!("cant apply `{got}` as procedure"),
            Self::WrongArgumentsCount{proc, this_invoked: _, expected, got} =>
            {
                let got = if let Some(got) = got
                {
                    format!(" (got {got})")
                } else
                {
                    String::new()
                };

                format!("wrong amount of arguments{got} passed to {proc} (expected {expected})")
            },
            Self::IndexOutOfRange(i) => format!("index {i} out of range"),
            Self::CharOutOfRange => "char out of range".to_owned(),
            Self::EmptySequence => "empty sequence".to_owned(),
            Self::VectorWrongType{expected, got} =>
                format!("vector expected `{expected:?}` got `{got:?}`"),
            Self::OperationError{a, b} =>
                format!("numeric error with {a} and {b} operands"),
            Self::ExpectedList => "expected a list".to_owned(),
            Self::ExpectedParam => "expected a parameter".to_owned(),
            Self::ExpectedOp => "expected an operator".to_owned(),
            Self::ExpectedClose => "expected a closing parenthesis".to_owned(),
            Self::ExpectedSymbol => "expected a valid symbol".to_owned(),
            Self::UnexpectedClose => "unexpected closing parenthesis".to_owned(),
            Self::UnexpectedEndOfFile => "unexpected end of file".to_owned()
        };

        write!(f, "{}", s)
    }
}

pub fn clone_with_capacity<T: Clone>(v: &Vec<T>) -> Vec<T>
{
    transfer_with_capacity(v, |x| x.clone())
}

pub fn transfer_with_capacity<T>(v: &Vec<T>, f: impl Fn(&T) -> T) -> Vec<T>
{
    let mut new_v = Vec::with_capacity(v.capacity());

    new_v.extend(v.iter().map(f));

    new_v
}

struct MemoryBlockWith<'a>
{
    memory: Option<&'a LispMemory>,
    block: &'a MemoryBlock
}

impl<'a> Debug for MemoryBlockWith<'a>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        // all fields r 32 bits long and u32 has no invalid states
        let general = self.block.general.iter().map(|raw| unsafe{ raw.unsigned }).collect::<Vec<u32>>();

        let pv = |v: &LispValue|
        {
            let mut visited = Vec::new();
            v.maybe_to_string(&mut visited, self.memory, Some(self.block))
        };

        let cars = self.block.cars.iter().map(pv).collect::<Vec<_>>();
        let cdrs = self.block.cdrs.iter().map(pv).collect::<Vec<_>>();

        f.debug_struct("MemoryBlock")
            .field("cars", &cars)
            .field("cdrs", &cdrs)
            .field("general", &general)
            .finish()
    }
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
        MemoryBlockWith{memory: None, block: self}.fmt(f)
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

        Ok(vec.values.iter().map(|x| unsafe{ x.character }).collect())
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

        LispValue::new_list_id(id as u32)
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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolId(u32);

impl Display for SymbolId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Symbols
{
    mappings: HashMap<String, SymbolId>,
    current_id: u32
}

impl Symbols
{
    pub fn new() -> Self
    {
        Self{mappings: HashMap::new(), current_id: 0}
    }

    pub fn get_by_name(&self, value: &str) -> Option<SymbolId>
    {
        self.mappings.get(value).copied()
    }

    pub fn get_by_id(&self, id: SymbolId) -> &str
    {
        self.mappings.iter().find_map(|(key, value)|
        {
            (*value == id).then_some(key)
        }).expect("all ids must be valid")
    }

    pub fn push(&mut self, value: String) -> SymbolId
    {
        if let Some(&id) = self.mappings.get(&value)
        {
            return id;
        }

        let id = SymbolId(self.current_id);
        self.current_id += 1;

        self.mappings.insert(value, id);

        id
    }
}

pub struct LispMemory
{
    quoted: Vec<LispValue>,
    symbols: Symbols,
    memory: MemoryBlock,
    swap_memory: MemoryBlock,
    stack: Vec<LispValue>,
    registers: [LispValue; Register::COUNT]
}

impl Debug for LispMemory
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let pv = |v: LispValue|
        {
            v.to_string(self)
        };

        let quoted = self.quoted.iter().copied().map(pv).collect::<Vec<_>>();
        let stack = self.stack.iter().copied().map(pv).collect::<Vec<_>>();
        let registers = self.registers.map(pv);

        let block = MemoryBlockWith{memory: Some(self), block: &self.memory};
        f.debug_struct("MemoryBlock")
            .field("quoted", &quoted)
            .field("symbols", &self.symbols)
            .field("memory", &block)
            .field("stack", &stack)
            .field("registers", &registers)
            .finish()
    }
}

impl Clone for LispMemory
{
    fn clone(&self) -> Self
    {
        Self{
            quoted: self.quoted.clone(),
            symbols: self.symbols.clone(),
            memory: self.memory.clone(),
            swap_memory: self.swap_memory.clone(),
            stack: clone_with_capacity(&self.stack),
            registers: self.registers.clone()
        }
    }
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
        let memory = MemoryBlock::new(memory_size);
        let swap_memory = MemoryBlock::new(memory_size);

        let mut this = Self{
            quoted: Vec::new(),
            symbols: Symbols::new(),
            memory,
            swap_memory,
            stack: Vec::with_capacity(stack_size),
            registers: [LispValue::new_empty_list(); Register::COUNT]
        };

        let env = this.create_env(()).expect("must have enough memory for default env");
        this.set_register(Register::Environment, env);

        this
    }

    pub fn create_env(&mut self, parent: impl Into<LispValue>) -> Result<LispValue, Error>
    {
        self.set_register(Register::Value, ());
        self.set_register(Register::Temporary, parent);

        self.cons(Register::Temporary, Register::Value, Register::Temporary)?;

        self.set_register(Register::Value, LispValue::new_environment_marker());

        self.cons(Register::Value, Register::Value, Register::Temporary)?;

        Ok(self.get_register(Register::Value))
    }

    pub fn no_memory() -> Self
    {
        Self::new(256, 0)
    }

    pub fn clear(&mut self)
    {
        self.memory.clear();
    }

    pub fn get_symbol_by_name(&self, name: &str) -> Option<SymbolId>
    {
        self.symbols.get_by_name(name)
    }
    pub fn add_quoted(&mut self, value: LispValue)
    {
        self.quoted.push(value);
    }

    pub fn define(&mut self, key: impl Into<String>, value: LispValue) -> Result<(), Error>
    {
        let symbol = self.new_symbol(key.into());

        self.define_symbol(symbol.as_symbol_id().expect("must be a symbol"), value)
    }

    pub fn define_symbol(&mut self, key: SymbolId, value: LispValue) -> Result<(), Error>
    {
        if let Some(id) = self.lookup_in_env_id::<false>(
            self.get_register(Register::Environment),
            key
        )
        {
            self.set_cdr(id, value);
            return Ok(());
        }

        let mappings_id = |this: &Self|
        {
            let pair = this.get_register(Register::Environment).as_list(this).expect("env must be a list").cdr;
            pair.as_list_id().expect("env cdr must be list")
        };

        {
            let restore = self.with_saved_registers([Register::Value, Register::Temporary]);

            self.set_register(Register::Value, LispValue::new_symbol_id(key));
            self.set_register(Register::Temporary, value);
            self.cons(Register::Value, Register::Value, Register::Temporary)?;

            let tail = self.get_car(mappings_id(self));

            self.set_register(Register::Temporary, tail);

            self.cons(Register::Value, Register::Value, Register::Temporary)?;

            let new_env = self.get_register(Register::Value);

            self.set_car(mappings_id(self), new_env);

            restore(self);
        }

        Ok(())
    }

    pub fn with_saved_registers(
        &mut self,
        registers: impl IntoIterator<Item=Register> + Clone
    ) -> impl FnOnce(&mut LispMemory)
    {
        registers.clone().into_iter().for_each(|register|
        {
            self.push_stack_register(register);
        });

        move |memory|
        {
            registers.into_iter().for_each(|register|
            {
                memory.pop_stack_register(register);
            });
        }
    }

    pub fn lookup(&self, name: &str) -> Option<LispValue>
    {
        let symbol = self.symbols.get_by_name(name)?;
        self.lookup_symbol(symbol)
    }

    pub fn lookup_symbol(&self, symbol: SymbolId) -> Option<LispValue>
    {
        self.lookup_in_env::<true>(self.get_register(Register::Environment), symbol)
    }

    fn lookup_in_env<const CHECK_PARENT: bool>(
        &self,
        env: LispValue,
        symbol: SymbolId
    ) -> Option<LispValue>
    {
        let id = self.lookup_in_env_id::<CHECK_PARENT>(env, symbol);

        id.map(|id| self.get_cdr(id))
    }

    fn lookup_in_env_id<const CHECK_PARENT: bool>(
        &self,
        env: LispValue,
        symbol: SymbolId
    ) -> Option<u32>
    {
        if env.is_null()
        {
            return None;
        }

        let pair = env.as_list(self).expect("env must be a list").cdr
            .as_list(self).expect("env cdr must be a list");

        let parent = pair.cdr;
        let mut maybe_mappings = pair.car;

        let id = loop
        {
            if maybe_mappings.is_null()
            {
                return CHECK_PARENT.then(||
                {
                    self.lookup_in_env_id::<CHECK_PARENT>(parent, symbol)
                }).flatten();
            }

            let mappings = maybe_mappings.as_list(self).expect("env must be a list");

            if let Some(x) = self.lookup_pair(*mappings.car(), symbol)
            {
                break x;
            }

            maybe_mappings = *mappings.cdr();
        };

        Some(id)
    }

    fn lookup_pair(&self, pair: LispValue, symbol: SymbolId) -> Option<u32>
    {
        let id = pair.as_list_id().expect("must be a list");

        let symbol_id = self.get_car(id).as_symbol_id().expect("must be symbol");

        (symbol_id == symbol).then_some(id)
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

                create_id(new_id)
            },
            _ => value
        }
    }

    fn transfer_stacks(&mut self)
    {
        macro_rules! transfer_stack
        {
            ($name:ident) =>
            {
                self.$name.iter_mut().for_each(|value|
                {
                    *value = Self::transfer_to_swap_value(
                        &mut self.memory,
                        &mut self.swap_memory,
                        *value
                    );
                });
            }
        }

        transfer_stack!(stack);
        transfer_stack!(quoted);
        transfer_stack!(registers);
    }

    pub fn gc(&mut self)
    {
        self.transfer_stacks();

        let transfer_swap = |
            this: &mut Self,
            general_scan: &mut usize,
            cars_scan: &mut usize,
            cdrs_scan: &mut usize
        |
        {
            macro_rules! transfer_memory
            {
                ($part:ident, $scan:expr) =>
                {
                    while *$scan < this.swap_memory.$part.len()
                    {
                        let value = this.swap_memory.$part[*$scan];
                        this.swap_memory.$part[*$scan] = Self::transfer_to_swap_value(
                            &mut this.memory,
                            &mut this.swap_memory,
                            value
                        );

                        *$scan += 1;
                    }
                }
            }

            let mut general_tag = ValueTag::Integer;
            let mut general_remaining = 0;

            while *cars_scan < this.swap_memory.cars.len()
                || *cdrs_scan < this.swap_memory.cdrs.len()
                || *general_scan < this.swap_memory.general.len()
            {
                transfer_memory!(cars, cars_scan);
                transfer_memory!(cdrs, cdrs_scan);

                while *general_scan < this.swap_memory.general.len()
                {
                    if general_remaining == 0
                    {
                        (general_tag, general_remaining) = this.swap_memory
                            .vector_raw_info(*general_scan as u32);

                        *general_scan += 2;
                    }

                    if general_remaining > 0
                    {
                        if general_tag.is_boxed()
                        {
                            let value = this.swap_memory.general[*general_scan];
                            this.swap_memory.general[*general_scan] = Self::transfer_to_swap_value(
                                &mut this.memory,
                                &mut this.swap_memory,
                                LispValue{tag: general_tag, value}
                            ).value;

                            general_remaining -= 1;
                            *general_scan += 1;
                        } else
                        {
                            *general_scan += general_remaining;
                            general_remaining = 0;
                        }
                    }
                }
            }

            debug_assert!(general_remaining == 0);
        };

        let mut general_scan = 0;
        let mut cars_scan = 0;
        let mut cdrs_scan = 0;

        transfer_swap(
            self,
            &mut general_scan,
            &mut cars_scan,
            &mut cdrs_scan
        );

        mem::swap(&mut self.memory, &mut self.swap_memory);

        self.swap_memory.clear();
    }

    fn stack_push(stack: &mut Vec<LispValue>, value: LispValue)
    {
        if stack.len() == stack.capacity()
        {
            panic!("stack overflow!!!! ahhhh!!");
        }

        stack.push(value);
    }

    pub fn pop_arg(&mut self) -> LispValue
    {
        self.try_pop_arg().expect("cant get more arguments than stored")
    }

    pub fn try_pop_arg(&mut self) -> Option<LispValue>
    {
        let pair = self.get_register(Register::Argument);
        if pair.is_null()
        {
            return None;
        }

        let LispList{car, cdr} = pair.as_list(self).expect("arg register must contain a pair");
        self.set_register(Register::Argument, cdr);

        Some(car)
    }

    pub fn get_register(&self, register: Register) -> LispValue
    {
        self.registers[register as usize]
    }

    pub fn set_register(&mut self, register: Register, value: impl Into<LispValue>)
    {
        self.registers[register as usize] = value.into();
    }

    pub fn is_empty_args(&self) -> bool
    {
        self.get_register(Register::Argument).is_null()
    }

    pub fn push_stack_register(&mut self, register: Register)
    {
        self.push_stack(self.registers[register as usize]);
    }

    pub fn pop_stack_register(&mut self, register: Register)
    {
        self.registers[register as usize] = self.pop_stack();
    }

    pub fn push_stack(&mut self, value: impl Into<LispValue>)
    {
        Self::stack_push(&mut self.stack, value.into())
    }

    pub fn pop_stack(&mut self) -> LispValue
    {
        self.try_pop_stack().expect("cant pop from an empty stack, how did this happen?")
    }

    pub fn try_pop_stack(&mut self) -> Option<LispValue>
    {
        self.stack.pop()
    }

    pub fn stack_len(&self) -> usize
    {
        self.stack.len()
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

    pub fn get_symbol(&self, id: SymbolId) -> String
    {
        self.symbols.get_by_id(id).to_owned()
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
        self.memory.get_cdr(id)
    }

    pub fn set_car(&mut self, id: u32, value: LispValue)
    {
        self.memory.set_car(id, value)
    }

    pub fn set_cdr(&mut self, id: u32, value: LispValue)
    {
        self.memory.set_cdr(id, value)
    }

    fn need_list_memory(&mut self, amount: usize) -> Result<(), Error>
    {
        if self.memory.list_remaining() < amount
        {
            self.gc();

            if self.memory.list_remaining() < amount
            {
                return Err(Error::OutOfMemory);
            }
        }

        Ok(())
    }

    fn need_memory(&mut self, amount: usize) -> Result<(), Error>
    {
        if self.memory.remaining() < amount
        {
            self.gc();

            if self.memory.remaining() < amount
            {
                return Err(Error::OutOfMemory);
            }
        }

        Ok(())
    }

    pub fn cons(&mut self, target: Register, car: Register, cdr: Register) -> Result<(), Error>
    {
        self.need_list_memory(1)?;

        let car = self.get_register(car);
        let cdr = self.get_register(cdr);

        let pair = self.memory.cons(car, cdr);
        self.set_register(target, pair);

        Ok(())
    }

    pub fn cons_list<I, V>(
        &mut self,
        values: I
    ) -> Result<(), Error>
    where
        V: Into<LispValue>,
        I: IntoIterator<Item=V>,
        I::IntoIter: DoubleEndedIterator + ExactSizeIterator
    {
        /*let iter = values.into_iter();
        let len = iter.len();

        iter.for_each(|x| self.push_stack(x));

        self.push_stack(());

        (0..len).try_for_each(|_| self.cons())*/todo!()
    }

    pub fn new_primitive_value(&mut self, x: PrimitiveType) -> LispValue
    {
        match x
        {
            PrimitiveType::Value(x) => self.new_symbol(x),
            PrimitiveType::Char(x) => LispValue::new_char(x),
            PrimitiveType::Float(x) => LispValue::new_float(x),
            PrimitiveType::Integer(x) => LispValue::new_integer(x),
            PrimitiveType::Bool(x) => LispValue::new_bool(x)
        }
    }

    pub fn new_symbol(
        &mut self,
        x: impl Into<String>
    ) -> LispValue
    {
        let x = x.into();

        let id = self.symbols.push(x);

        LispValue::new_symbol_id(id)
    }

    pub fn make_vector(
        &mut self,
        target: Register,
        vec: LispVectorInner<&[ValueRaw]>
    ) -> Result<(), Error>
    {
        let len = vec.values.len();

        // +2 for the length and for the type tag
        self.need_memory(len + 2)?;

        let id = self.memory.allocate_iter(vec.tag, vec.values.iter());

        self.set_register(target, LispValue::new_vector(id));

        Ok(())
    }
}

pub struct GenericOutputWrapper<M>
{
    memory: M,
    pub value: LispValue
}

pub type OutputWrapperRef<'a> = GenericOutputWrapper<&'a LispMemory>;
pub type OutputWrapper = GenericOutputWrapper<LispMemory>;

impl OutputWrapper
{
    pub fn as_list(&self) -> Result<LispList<OutputWrapperRef>, Error>
    {
        let lst = self.value.as_list(&self.memory)?;

        Ok(lst.map_ref(|value|
        {
            OutputWrapperRef{memory: &self.memory, value: *value}
        }))
    }
}

impl<'a> Clone for OutputWrapperRef<'a>
{
    fn clone(&self) -> Self
    {
        Self{
            memory: self.memory,
            value: self.value
        }
    }
}

impl<'a> OutputWrapperRef<'a>
{
    pub fn as_list(&self) -> Result<LispList<OutputWrapperRef<'a>>, Error>
    {
        let lst = self.value.as_list(self.memory)?;

        Ok(lst.map_ref(|value|
        {
            OutputWrapperRef{memory: self.memory, value: *value}
        }))
    }
}

impl<M: Borrow<LispMemory>> Display for GenericOutputWrapper<M>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.value.to_string(self.memory.borrow()))
    }
}

impl<M> Deref for GenericOutputWrapper<M>
{
    type Target = LispValue;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<M> GenericOutputWrapper<M>
{
    pub fn into_value(self) -> LispValue
    {
        self.value
    }

    pub fn into_memory(self) -> M
    {
        self.memory
    }

    pub fn destructure(self) -> (M, LispValue)
    {
        (self.memory, self.value)
    }
}

impl<M: Borrow<LispMemory>> GenericOutputWrapper<M>
{
    pub fn as_vector_ref(&self) -> Result<LispVectorRef, Error>
    {
        self.value.as_vector_ref(self.memory.borrow())
    }

    pub fn as_vector(&self) -> Result<LispVector, Error>
    {
        self.value.as_vector(self.memory.borrow())
    }

    pub fn as_symbol(&self) -> Result<String, Error>
    {
        self.value.as_symbol(self.memory.borrow())
    }
}

pub struct LispConfig
{
    pub primitives: Rc<Primitives>,
    pub type_checks: bool,
    pub memory: LispMemory
}

#[derive(Debug)]
pub struct Lisp
{
    program: Program
}

impl Lisp
{
    pub fn new_with_config(
        config: LispConfig,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        let program = Program::parse(
            config.primitives,
            config.type_checks,
            config.memory,
            code
        )?;

        Ok(Self{program})
    }

    pub fn new_with_memory(
        memory: LispMemory,
        code: &str
    ) -> Result<Self, ErrorPos>
    {
        let config = LispConfig{
            primitives: Rc::new(Primitives::new()),
            type_checks: true,
            memory
        };

        Self::new_with_config(config, code)
    }

    pub fn new(code: &str) -> Result<Self, ErrorPos>
    {
        Self::new_with_memory(Self::default_memory(), code)
    }

    pub fn memory_mut(&mut self) -> &mut LispMemory
    {
        self.program.memory_mut()
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
        self.program.eval()
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    fn simple_integer_test(code: &str, result: i32)
    {
        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap_or_else(|err|
        {
            panic!("{err}")
        });

        let value = value.as_integer().unwrap_or_else(|err|
        {
            panic!("{err} ({value})")
        });

        assert_eq!(value, result);
    }

    #[test]
    fn ycomb_factorial()
    {
        let code = |number|
        {
            format!("
            ((lambda (x)
                    ((lambda (f) (f f x))
                        (lambda (f n)
                            (if (= n 1)
                                1
                                (* n (f f (- n 1)))))))
                {number})")
        };

        simple_integer_test(&code(1), 1);
        simple_integer_test(&code(2), 2);
        simple_integer_test(&code(7), 5040);
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

        simple_integer_test(code, 10);
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

        simple_integer_test(code, 5040);
    }

    #[test]
    fn let_thingy()
    {
        let code = "
            (let ((x (+ 2 1)) (y (* 4 7)))
                (let ((z (+ x y)))
                    (+ z y x)))
        ";

        // simple math too hard for me sry
        simple_integer_test(code, 34_i32 + 28);
    }

    #[test]
    fn define_lambdas()
    {
        let code = "
            (define one (lambda (x) (+ x 5)))
            (define two one)

            (+ (two 4) (one 5))
        ";

        simple_integer_test(code, 19);
    }

    #[test]
    fn define_many()
    {
        let code = " ; 1
            (define v (make-vector 2 0)) ; 2
            (define (thingy x) ; 3
                (vector-set! v 0 x) ; 4
                x) ; 5

            (define x (thingy 4)) ; 7
            (+ x (vector-ref v 0)) ; 8
        ";

        simple_integer_test(code, 8);
    }

    #[test]
    fn redefine_primitive()
    {
        let code = "
            (define cooler-cons cons)

            (define l (cooler-cons 5 3))

            (+ (car l) (cdr l))
        ";

        simple_integer_test(code, 8);
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

        let memory_size = 92;
        let memory = LispMemory::new(10, memory_size);

        let mut lisp = Lisp::new_with_memory(memory, code).unwrap();

        let value = lisp.run()
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

            (define (nothing)
                (+ (silly 7) (silly 5) (silly 6) (silly 11) (silly 4)))

            (nothing)
            (nothing)
            (nothing)

            (display b)

            (+ a b)
        ";

        let memory_size = 430;
        let memory = LispMemory::new(32, memory_size);

        let mut lisp = Lisp::new_with_memory(memory, code).unwrap();

        let value = lisp.run()
            .unwrap()
            .as_integer()
            .unwrap();

        assert_eq!(value, 39_922_707_i32);
    }

    fn list_equals(list: LispList<OutputWrapperRef>, check: &[i32])
    {
        let car = list.car().as_integer().unwrap();

        assert_eq!(car, check[0]);

        let check = &check[1..];
        if check.is_empty()
        {
            return;
        }

        list_equals(list.cdr().as_list().unwrap(), check)
    }

    #[test]
    fn quoting()
    {
        let code = "
            (quote (1 2 3 4 5))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();

        let value = output.as_list().unwrap();

        list_equals(value, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn quoting_tick_list()
    {
        let code = "
            '(1 2 3 4 5)
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();

        let value = output.as_list().unwrap();

        list_equals(value, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn quoting_tick()
    {
        let code = "
            'heyyy
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();

        let value = output.as_symbol().unwrap();

        assert_eq!(value, "heyyy".to_owned());
    }

    #[test]
    fn list()
    {
        let code = "
            (cons 3 (cons 4 (cons 5 (quote ()))))
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let output = lisp.run().unwrap();

        let value = output.as_list().unwrap();

        list_equals(value, &[3, 4, 5]);
    }

    #[test]
    fn carring()
    {
        let code = "
            (define x (cons 3 (cons 4 (cons 5 (quote ())))))

            (car (cdr (cdr x)))
        ";

        simple_integer_test(code, 5);
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

        let memory_size = 300;
        let memory = LispMemory::new(256, memory_size);

        let mut lisp = Lisp::new_with_memory(memory, &code).unwrap();

        let output = lisp.run().unwrap();

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

        simple_integer_test(code, 11);
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

        simple_integer_test(code, 6);
    }

    #[test]
    fn displaying_one()
    {
        let code = "
            (display 'hey)

            0
        ";

        simple_integer_test(code, 0);
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

        simple_integer_test(code, 0);
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

        simple_integer_test(code, 0);
    }

    #[test]
    fn booleans()
    {
        let code = "
            (if #t 2 3)
        ";

        simple_integer_test(code, 2);
    }

    #[test]
    fn self_eval()
    {
        let code = "
            12345
        ";

        simple_integer_test(code, 12345);
    }

    #[test]
    fn char()
    {
        let code = "
            #\\x
        ";

        let mut lisp = Lisp::new(code).unwrap();

        let value = lisp.run().unwrap().as_char().unwrap();

        assert_eq!(value, 'x');
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
            (define x           ; 2
                (lambda (x)     ; 3
                    (if (= x 1) ; 4
                        8       ; 5
                        2)))    ; 6

            (+ (x 1) (x 5))     ; 8
        ";

        simple_integer_test(code, 10);
    }

    #[test]
    fn if_no_else()
    {
        let code = "
            (if (null? (if (= 5 1) 8))
                1)
        ";

        simple_integer_test(code, 1);
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

        simple_integer_test(code, 2);
    }

    #[test]
    fn define()
    {
        let code = "
            (define x (+ 2 3))

            x
        ";

        simple_integer_test(code, 5);
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
    fn compound_args()
    {
        let code = "
            (+ (* 5 10) 4 (/ 100 2))
        ";

        simple_integer_test(code, 104);
    }

    #[test]
    fn addition()
    {
        let code = "
            (+ 3 6)
        ";

        simple_integer_test(code, 9);
    }

    #[test]
    fn subtraction()
    {
        let code = "
            (- 7 6)
        ";

        simple_integer_test(code, 1);
    }

    #[test]
    fn multi_addition()
    {
        let code = "
            (+ 1 2 3)
        ";

        simple_integer_test(code, 6);
    }

    #[test]
    fn weird_spacing_addition()
    {
        let code = "
            (+   1  2
              3

              )
        ";

        simple_integer_test(code, 6);
    }

    #[test]
    fn empty_quote()
    {
        let code = "
            (quote)
        ";

        let lisp = Lisp::new(code);

        assert!(lisp.is_err());
    }
}
