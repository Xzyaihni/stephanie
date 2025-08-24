use std::{
    iter,
    fmt::{self, Display},
    str::Chars
};

use super::{WithPosition, WithPositionTrait};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePosition
{
    pub source: usize,
    pub line: usize,
    pub character: usize
}

impl Default for CodePosition
{
    fn default() -> Self
    {
        Self::new(0)
    }
}

impl CodePosition
{
    pub fn new(source: usize) -> Self
    {
        Self{source, line: 1, character: 0}
    }

    pub fn next_char(&mut self)
    {
        self.character += 1;
    }

    pub fn next_line(&mut self)
    {
        self.line += 1;
        self.character = 0;
    }
}

impl Display for CodePosition
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}:{}", self.line, self.character)
    }
}

pub type LexemePos = WithPosition<Lexeme>;

#[derive(Debug, PartialEq, Eq)]
pub enum Lexeme
{
    OpenParen,
    CloseParen,
    Quote,
    Value(String)
}

pub struct Lexer<'a>
{
    position: CodePosition,
    chars: Chars<'a>,
    current_char: Option<char>
}

impl<'a> Lexer<'a>
{
    pub fn parse(texts: &[&'a str]) -> Vec<LexemePos>
    {
        texts.iter().enumerate().flat_map(|(source, text)|
        {
            let this = Self{
                position: CodePosition::new(source),
                chars: text.chars(),
                current_char: None
            };

            this.parse_lexemes()
        }).collect()
    }

    // wow not returning an iterator? so inefficient wow wow wow
    fn parse_lexemes(mut self) -> Vec<LexemePos>
    {
        iter::from_fn(|| self.parse_one()).collect()
    }

    fn parse_one(&mut self) -> Option<LexemePos>
    {
        let position = self.position;
        let mut current = String::new();
        let mut comment = false;
        let mut is_string = false;
        let mut is_escape = false;

        loop
        {
            if let Some(c) = self.next_char()
            {
                let unescaped = move |check|
                {
                    !is_escape && c == check
                };

                if is_escape
                {
                    is_escape = false;
                }

                if c == '\n'
                {
                    self.position.next_line();

                    if comment
                    {
                        self.consume_char();

                        comment = false;
                        continue;
                    }
                }

                if unescaped('\\') && current.chars().next().map(|x| x != '#').unwrap_or(true)
                {
                    self.consume_char();
                    is_escape = true;
                    continue;
                }

                if !comment
                {
                    if unescaped('"')
                    {
                        if is_string
                        {
                            self.consume_char();

                            return Some(Lexeme::Value(current).with_position(position));
                        } else
                        {
                            if current.is_empty()
                            {
                                self.consume_char();
                                current.push(c);

                                is_string = true;
                                continue;
                            }

                            return Some(Lexeme::Value(current).with_position(position));
                        }
                    }

                    if is_string
                    {
                        self.consume_char();
                        current.push(c);

                        continue;
                    }
                }

                if comment || unescaped(';')
                {
                    if current.is_empty()
                    {
                        self.consume_char();
                        comment = true;
                        continue;
                    }

                    return Some(Lexeme::Value(current).with_position(position));
                }

                if c == '\''
                {
                    if current.is_empty()
                    {
                        self.consume_char();

                        return Some(Lexeme::Quote.with_position(position));
                    }

                    return Some(Lexeme::Value(current).with_position(position));
                }

                if (c == '(') || (c == ')')
                {
                    if current.is_empty()
                    {
                        let position = self.position;
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

                        return Some(lexeme.with_position(position));
                    }

                    return Some(Lexeme::Value(current).with_position(position));
                }

                if !current.is_empty() || !c.is_whitespace()
                {
                    if let Some(last_c) = current.chars().last()
                    {
                        if last_c.is_whitespace() && !c.is_whitespace()
                        {
                            return Some(Lexeme::Value(current).with_position(position));
                        } else
                        {
                            current.push(c);
                        }
                    } else
                    {
                        current.push(c);
                    }
                }

                self.consume_char();
            } else
            {
                if current.is_empty()
                {
                    return None;
                }

                return Some(Lexeme::Value(current).with_position(self.position));
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
        self.position.next_char();

        self.current_char = None;
    }
}
