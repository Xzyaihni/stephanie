use std::{
    iter,
    vec,
    fmt::{self, Display},
    ops::Deref
};

use super::{BEGIN_PRIMITIVE, QUOTE_PRIMITIVE, Error, ErrorPos};

pub use lexer::CodePosition;

use lexer::{Lexer, Lexeme, LexemePos};

mod lexer;


#[derive(Debug, Clone)]
pub enum PrimitiveType
{
    Value(String),
    Char(char),
    Float(f32),
    Integer(i32),
    Bool(bool)
}

impl Display for PrimitiveType
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", match self
        {
            Self::Value(x) =>
            {
                return write!(f, "{x}");
            },
            Self::Char(x) => x.to_string(),
            Self::Float(x) => x.to_string(),
            Self::Integer(x) => x.to_string(),
            Self::Bool(x) => x.to_string()
        })
    }
}

pub type AstPos = WithPosition<Ast>;

impl AstPos
{
    pub fn cons(car: Self, cdr: Self) -> Self
    {
        Self{
            position: car.position,
            value: Ast::List{car: Box::new(car), cdr: Box::new(cdr)}
        }
    }

    pub fn as_value(&self) -> Result<PrimitiveType, ErrorPos>
    {
        match &self.value
        {
            Ast::Value(x) => Ast::parse_primitive(x)
                .map_err(|value| ErrorPos{position: self.position, value}),
            x => panic!("as_value must be called on a value, called on {x:?}")
        }
    }

    pub fn map(self, f: impl FnOnce(Ast) -> Ast) -> Self
    {
        Self{
            value: f(self.value),
            ..self
        }
    }

    pub fn map_list<F>(&self, mut f: F) -> Self
    where
        F: FnMut(Self) -> Self
    {
        if self.is_null()
        {
            return self.clone();
        }

        let car = self.car();
        let cdr = self.cdr();

        AstPos{
            position: self.position,
            value: Ast::List{
                car: Box::new(f(car)),
                cdr: Box::new(cdr.map_list(f))
            }
        }
    }

    pub fn list_to_vec(self) -> Vec<Self>
    {
        if !self.is_list()
        {
            panic!("list to vec must be called on a list");
        }

        if self.is_null()
        {
            Vec::new()
        } else
        {
            iter::once(self.car()).chain(self.cdr().list_to_vec()).collect()
        }
    }
}

// the most inefficient implementation of this ever!
fn parse_char_inner(chars: &mut impl ExactSizeIterator<Item=char>) -> String
{
    if chars.len() <= 1
    {
        chars.next().into_iter().collect()
    } else
    {
        #[allow(clippy::collapsible_else_if)]
        if chars.by_ref().next().unwrap().is_whitespace()
        {
            parse_char_inner(chars)
        } else
        {
            chars.collect()
        }
    }
}

fn parse_char(s: &str) -> String
{
    let chars: Vec<char> = s.chars().rev().collect();
    let mut chars = chars.into_iter();

    parse_char_inner(&mut chars).chars().rev().collect()
}

#[derive(Debug, Clone)]
pub enum Ast
{
    Value(String),
    EmptyList,
    List{car: Box<AstPos>, cdr: Box<AstPos>}
}

impl Ast
{
    pub fn to_string_pretty(&self) -> String
    {
        match self
        {
            Self::EmptyList => "()".to_owned(),
            Self::Value(x) => Self::parse_primitive(x).map_or_else(|_| x.clone(), |x| x.to_string()),
            Self::List{car, cdr} => format!(
                "({} {})",
                car.to_string_pretty(),
                cdr.to_string_pretty()
            )
        }
    }

    pub fn car(&self) -> AstPos
    {
        match self
        {
            Self::List{car, ..} => *car.clone(),
            x => panic!("car must be called on a list, called on {x:?}")
        }
    }

    pub fn cdr(&self) -> AstPos
    {
        match self
        {
            Self::List{cdr, ..} => *cdr.clone(),
            x => panic!("cdr must be called on a list, called on {x:?}")
        }
    }

    pub fn parse_primitive(x: &str) -> Result<PrimitiveType, Error>
    {
        if let Some(special) = x.strip_prefix('#')
        {
            if let Some(c) = special.strip_prefix('\\')
            {
                let parsed = parse_char(c);

                return if parsed.len() == 1
                {
                    Ok(PrimitiveType::Char(parsed.chars().next().unwrap()))
                } else
                {
                    Err(Error::CharTooLong(parsed.to_owned()))
                };
            }

            let x = match special.trim()
            {
                "t" => true,
                "f" => false,
                unknown => return Err(Error::SpecialParse(unknown.to_owned()))
            };

            return Ok(PrimitiveType::Bool(x));
        }

        let mut x = x.trim();

        let stripped = x.strip_prefix('-');

        let negative = stripped.is_some();

        if let (true, Some(stripped)) = (x.len() > 1, stripped)
        {
            x = stripped;
        }

        if x.starts_with(|c: char| !c.is_ascii_digit())
        {
            return Ok(PrimitiveType::Value(x.to_owned()));
        }

        let out = if x.contains('.')
        {
            let n: f32 = x.parse().map_err(|_| Error::NumberParse(x.to_owned()))?;

            PrimitiveType::Float(if negative { -n } else { n })
        } else
        {
            let n: i32 = x.parse().map_err(|_| Error::NumberParse(x.to_owned()))?;

            PrimitiveType::Integer(if negative { -n } else { n })
        };

        Ok(out)
    }

    pub fn is_list(&self) -> bool
    {
        match self
        {
            Self::List{..} | Self::EmptyList => true,
            _ => false
        }
    }

    pub fn list_length(&self) -> usize
    {
        if !self.is_list()
        {
            panic!("list length must be called on a list");
        }

        if self.is_null()
        {
            0
        } else
        {
            1 + self.cdr().list_length()
        }
    }

    #[allow(dead_code)]
    pub fn is_null(&self) -> bool
    {
        match self
        {
            Self::EmptyList => true,
            _ => false
        }
    }

    #[allow(dead_code)]
    pub fn is_last_list(&self) -> bool
    {
        match self
        {
            Self::List{cdr, ..} =>
            {
                cdr.is_null()
            },
            x => panic!("is_last_list must be called on a list, called on {x:?}")
        }
    }
}


#[derive(Debug, Clone, Copy)]
pub struct WithPosition<T>
{
    pub position: CodePosition,
    pub value: T
}

impl<T> WithPositionTrait<WithPosition<T>> for T
{
    fn with_position(self, position: CodePosition) -> WithPosition<T>
    {
        WithPosition{position, value: self}
    }
}

impl<T> Deref for WithPosition<T>
{
    type Target = T;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WithPositionMaybe<T>
{
    pub position: Option<CodePosition>,
    pub value: T
}

impl<T> From<T> for WithPositionMaybe<T>
{
    fn from(value: T) -> Self
    {
        Self{position: None, value}
    }
}

impl<T> Deref for WithPositionMaybe<T>
{
    type Target = T;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<T> WithPositionTrait<WithPositionMaybe<T>> for T
{
    fn with_position(self, position: CodePosition) -> WithPositionMaybe<T>
    {
        WithPositionMaybe{position: Some(position), value: self}
    }
}

pub trait WithPositionTrait<T>
{
    fn with_position(self, position: CodePosition) -> T;
}

pub struct Parser
{
    current_position: CodePosition,
    // of course i have to spell out the whole iterator type
    lexemes: iter::Chain<
        iter::Chain<
            iter::Once<LexemePos>,
            vec::IntoIter<LexemePos>>,
        iter::Once<LexemePos>>
}

impl Parser
{
    pub fn parse(code: &str) -> Result<AstPos, ErrorPos>
    {
        let lexemes = Lexer::parse(code);

        let lexemes = iter::once(Lexeme::OpenParen.with_position(Default::default()))
            .chain(lexemes)
            .chain(iter::once(Lexeme::CloseParen.with_position(Default::default())));

        let mut this = Self{current_position: CodePosition::new(), lexemes};

        let (pos, ast) = this.parse_one()?;

        let ast = AstPos{
            position: pos,
            value: ast.ok_or(ErrorPos{position: pos, value: Error::UnexpectedClose})?
        };

        Ok(Ast::List{
            car: AstPos{position: Default::default(), value: Ast::Value(BEGIN_PRIMITIVE.to_owned())}.into(),
            cdr: ast.into()
        }.with_position(Default::default()))
    }

    fn parse_one(&mut self) -> Result<(CodePosition, Option<Ast>), ErrorPos>
    {
        let lexeme = self.lexemes.next()
            .ok_or(ErrorPos{position: self.current_position, value: Error::ExpectedClose})?;

        let position = lexeme.position;
        self.current_position = position;

        let pair = match lexeme.value
        {
            Lexeme::Value(x) =>
            {
                (position, Some(Ast::Value(x)))
            },
            Lexeme::Quote =>
            {
                let (position, rest) = self.parse_one()?;

                let rest = rest.ok_or(ErrorPos{position, value: Error::ExpectedClose})?;

                let rest = Ast::List{
                    car: Box::new(AstPos{position, value: rest}),
                    cdr: Box::new(AstPos{position, value: Ast::EmptyList})
                };

                let car = Box::new(AstPos{position, value: Ast::Value(QUOTE_PRIMITIVE.to_owned())});
                let cdr = Box::new(AstPos{position, value: rest});

                (position, Some(Ast::List{car, cdr}))
            },
            Lexeme::OpenParen =>
            {
                let lst = self.parse_list()?;
                (position, Some(lst.value))
            },
            Lexeme::CloseParen =>
            {
                (position, None)
            }
        };

        Ok(pair)
    }

    fn parse_list(&mut self) -> Result<AstPos, ErrorPos>
    {
        let (pos, car) = self.parse_one()?;

        if let Some(car) = car
        {
            self.parse_list_with_car(AstPos{position: pos, value: car})
        } else
        {
            Ok(AstPos{
                position: pos,
                value: Ast::EmptyList
            })
        }
    }

    fn parse_list_with_car(&mut self, car: AstPos) -> Result<AstPos, ErrorPos>
    {
        let car = Box::new(car);
        let (pos, cdr) = self.parse_one()?;

        let (car, cdr) = if let Some(cdr) = cdr
        {
            let new_cdr = self.parse_list_with_car(AstPos{position: pos, value: cdr})?;

            (car, new_cdr)
        } else
        {
            let cdr = AstPos{
                position: pos,
                value: Ast::EmptyList
            };

            (car, cdr)
        };

        Ok(AstPos{
            position: car.position,
            value: Ast::List{car, cdr: Box::new(cdr)}
        })
    }
}
