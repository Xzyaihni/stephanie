use std::{iter, vec, ops::Deref};

use super::{Error, ErrorPos};

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

#[derive(Debug, Clone)]
pub struct AstPos
{
    pub position: CodePosition,
    pub ast: Ast
}

impl AstPos
{
    pub fn cons(car: Self, cdr: Self) -> Self
    {
        Self{
            position: car.position,
            ast: Ast::List{car: Box::new(car), cdr: Box::new(cdr)}
        }
    }

    pub fn as_value(&self) -> Result<PrimitiveType, ErrorPos>
    {
        match &self.ast
        {
            Ast::Value(x) => Ast::parse_primitive(x)
                .map_err(|error| ErrorPos{position: self.position, error}),
            x => panic!("as_value must be called on a value, called on {x:?}")
        }
    }

    pub fn map(self, f: impl FnOnce(Ast) -> Ast) -> Self
    {
        Self{
            ast: f(self.ast),
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
            ast: Ast::List{
                car: Box::new(f(car)),
                cdr: Box::new(cdr.map_list(f))
            }
        }
    }
}

impl Deref for AstPos
{
    type Target = Ast;

    fn deref(&self) -> &Self::Target
    {
        &self.ast
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
        if chars.by_ref().next().unwrap().is_whitespace()
        {
            parse_char_inner(chars)
        } else
        {
            return chars.collect();
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
            Self::Value(x) => x.clone(),
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

pub trait WithPosition
{
    type Output: Sized;

    fn with_position(self, position: CodePosition) -> Self::Output;
}

impl WithPosition for Option<Ast>
{
    type Output = Result<AstPos, ErrorPos>;

    fn with_position(self, position: CodePosition) -> Self::Output
    {
        Ok(AstPos{
            position,
            ast: self.ok_or(ErrorPos{position, error: Error::UnexpectedClose})?
        })
    }
}

impl WithPosition for Ast
{
    type Output = AstPos;

    fn with_position(self, position: CodePosition) -> Self::Output
    {
        AstPos{
            position,
            ast: self
        }
    }
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

        let open = LexemePos{position: CodePosition::new(), lexeme: Lexeme::OpenParen};
        let close = LexemePos{position: CodePosition::new(), lexeme: Lexeme::CloseParen};

        let lexemes = iter::once(open)
            .chain(lexemes)
            .chain(iter::once(close));

        let mut this = Self{current_position: CodePosition::new(), lexemes};

        let (pos, ast) = this.parse_one()?;

        ast.with_position(pos)
    }

    fn parse_one(&mut self) -> Result<(CodePosition, Option<Ast>), ErrorPos>
    {
        let position = self.current_position;
        let lexeme = self.lexemes.next().ok_or(ErrorPos{position, error: Error::ExpectedClose})?;

        let position = lexeme.position;

        let ast = match lexeme.lexeme
        {
            Lexeme::Value(x) =>
            {
                Some(Ast::Value(x))
            },
            Lexeme::Quote =>
            {
                let (_, rest) = self.parse_one()?;

                let rest = rest.with_position(position).map(|x| x.ast)?;

                let rest = Ast::List{
                    car: Box::new(AstPos{position, ast: rest}),
                    cdr: Box::new(AstPos{position, ast: Ast::EmptyList})
                };

                let car = Box::new(AstPos{position, ast: Ast::Value("quote".to_owned())});
                let cdr = Box::new(AstPos{position, ast: rest});

                Some(Ast::List{car, cdr})
            },
            Lexeme::OpenParen =>
            {
                Some(self.parse_list()?.ast)
            },
            Lexeme::CloseParen =>
            {
                None
            }
        };

        Ok((position, ast))
    }

    fn parse_list(&mut self) -> Result<AstPos, ErrorPos>
    {
        let (pos, car) = self.parse_one()?;

        if let Some(car) = car
        {
            self.parse_list_with_car(AstPos{position: pos, ast: car})
        } else
        {
            Ok(AstPos{
                position: pos,
                ast: Ast::EmptyList
            })
        }
    }

    fn parse_list_with_car(&mut self, car: AstPos) -> Result<AstPos, ErrorPos>
    {
        let car = Box::new(car);
        let (pos, cdr) = self.parse_one()?;

        let ast = if let Some(cdr) = cdr
        {
            let new_cdr = self.parse_list_with_car(AstPos{position: pos, ast: cdr})?;

            Ast::List{car, cdr: Box::new(new_cdr)}
        } else
        {
            let cdr = AstPos{
                position: pos,
                ast: Ast::EmptyList
            };

            Ast::List{car, cdr: Box::new(cdr)}
        };

        Ok(AstPos{
            position: pos,
            ast
        })
    }
}
