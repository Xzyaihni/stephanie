use std::{iter, vec, ops::Deref};

use super::{Error, ErrorPos};

pub use lexer::CodePosition;

use lexer::{Lexer, Lexeme, LexemePos};

mod lexer;


#[derive(Debug, Clone)]
pub enum PrimitiveType
{
    Value(String),
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

#[derive(Debug, Clone)]
pub enum Ast
{
    Value(String),
    EmptyList,
    List{car: Box<AstPos>, cdr: Box<AstPos>}
}

impl Ast
{
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
            let x = match special
            {
                "t" => true,
                "f" => false,
                unknown => return Err(Error::SpecialParse(unknown.to_owned()))
            };

            return Ok(PrimitiveType::Bool(x));
        }

        if x.starts_with(|c: char| !c.is_ascii_digit())
        {
            return Ok(PrimitiveType::Value(x.to_owned()));
        }

        let out = if x.contains('.')
        {
            PrimitiveType::Float(
                x.parse().map_err(|_| Error::NumberParse(x.to_owned()))?)
        } else
        {
            PrimitiveType::Integer(
                x.parse().map_err(|_| Error::NumberParse(x.to_owned()))?)
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
