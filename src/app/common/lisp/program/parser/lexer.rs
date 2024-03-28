use std::{
    iter,
    str::Chars
};


#[derive(Debug, PartialEq, Eq)]
pub enum Lexeme
{
    OpenParen,
    CloseParen,
    Value(String)
}

pub struct Lexer<'a>
{
    chars: Chars<'a>,
    current_char: Option<char>
}

impl<'a> Lexer<'a>
{
    pub fn parse(text: &'a str) -> Vec<Lexeme>
    {
        let this = Self{chars: text.chars(), current_char: None};

        this.parse_lexemes()
    }

    // wow not returning an iterator? so inefficient wow wow wow
    fn parse_lexemes(mut self) -> Vec<Lexeme>
    {
        iter::from_fn(|| self.parse_one()).collect()
    }

    fn parse_one(&mut self) -> Option<Lexeme>
    {
        let mut current = String::new();
        
        loop
        {
            if let Some(c) = self.next_char()
            {
                if c.is_whitespace()
                {
                    self.consume_char();

                    if current.is_empty()
                    {
                        continue;
                    } else
                    {
                        return Some(Lexeme::Value(current));
                    }
                }

                if (c == '(') || (c == ')')
                {
                    if current.is_empty()
                    {
                        self.consume_char();

                        let lexeme = if c == ')'
                        {
                            Lexeme::CloseParen
                        } else if c == '('
                        {
                            Lexeme::OpenParen
                        } else
                        {
                            unreachable!()
                        };

                        return Some(lexeme);
                    } else
                    {
                        return Some(Lexeme::Value(current));
                    }
                }

                current.push(c);
                self.consume_char();
            } else
            {
                if current.is_empty()
                {
                    return None;
                } else
                {
                    return Some(Lexeme::Value(current));
                }
            }
        }
    }

    fn next_char(&mut self) -> Option<char>
    {
        if self.current_char.is_some()
        {
            return self.current_char;
        }

        let c = self.chars.next();

        self.current_char = c;

        c
    }

    fn consume_char(&mut self)
    {
        self.current_char = None;
    }
}
