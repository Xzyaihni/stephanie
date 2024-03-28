use std::{iter, vec};

use super::Error;

use lexer::{Lexer, Lexeme};

mod lexer;


#[derive(Debug, Clone)]
pub enum PrimitiveType
{
    Value(String),
    Float(f32),
    Integer(i32)
}

#[derive(Debug, Clone)]
pub enum Ast
{
    Value(String),
    EmptyList,
    List{car: Box<Ast>, cdr: Box<Ast>}
}

impl Ast
{
    pub fn car(&self) -> Ast
    {
        match self
        {
            Self::List{car, ..} => *car.clone(),
            x => panic!("car must be called on a list, called on {x:?}")
        }
    }

    pub fn cdr(&self) -> Ast
    {
        match self
        {
            Self::List{cdr, ..} => *cdr.clone(),
            x => panic!("cdr must be called on a list, called on {x:?}")
        }
    }

    pub fn as_value(&self) -> Result<PrimitiveType, Error>
    {
        match self
        {
            Self::Value(x) =>
            {
                if x.starts_with(|c: char| !c.is_ascii_digit())
                {
                    return Ok(PrimitiveType::Value(x.clone()));
                }

                let out = if x.contains('.')
                {
                    PrimitiveType::Float(
                        x.parse().map_err(|_| Error::NumberParse(x.clone()))?)
                } else
                {
                    PrimitiveType::Integer(
                        x.parse().map_err(|_| Error::NumberParse(x.clone()))?)
                };

                Ok(out)
            },
            x => panic!("as_number must be called on a value, called on {x:?}")
        }
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

pub struct Parser
{
    // of course i have to spell out the whole iterator type
    lexemes: iter::Chain<
        iter::Chain<
            iter::Once<Lexeme>,
            vec::IntoIter<Lexeme>>,
        iter::Once<Lexeme>>
}

impl Parser
{
    pub fn parse(code: &str) -> Result<Ast, Error>
    {
        let lexemes = Lexer::parse(code);

        let lexemes = iter::once(Lexeme::OpenParen)
            .chain(lexemes.into_iter())
            .chain(iter::once(Lexeme::CloseParen));

        let mut this = Self{lexemes};

        let ast = this.parse_one()?.ok_or(Error::UnexpectedClose)?;
        Ok(ast)
    }

    fn parse_one(&mut self) -> Result<Option<Ast>, Error>
    {
        let lexeme = self.lexemes.next().ok_or(Error::ExpectedClose)?;

        match lexeme
        {
            Lexeme::Value(x) =>
            {
                Ok(Some(Ast::Value(x)))
            },
            Lexeme::OpenParen =>
            {
                self.parse_list().map(|x| Some(x))
            },
            Lexeme::CloseParen =>
            {
                Ok(None)
            }
        }
    }

    fn parse_list(&mut self) -> Result<Ast, Error>
    {
        let car = self.parse_one()?.map(|x| Box::new(x));

        if let Some(car) = car
        {
            self.parse_list_with_car(car)
        } else
        {
            Ok(Ast::EmptyList)
        }
    }

    fn parse_list_with_car(&mut self, car: Box<Ast>) -> Result<Ast, Error>
    {
        let cdr = self.parse_one()?.map(|x| Box::new(x));

        if let Some(cdr) = cdr
        {
            let new_cdr = self.parse_list_with_car(cdr)?;

            Ok(Ast::List{car, cdr: Box::new(new_cdr)})
        } else
        {
            Ok(Ast::List{car, cdr: Box::new(Ast::EmptyList)})
        }
    }
}
