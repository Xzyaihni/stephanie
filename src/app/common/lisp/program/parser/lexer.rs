use std::{
    iter,
    fmt::{self, Display},
    str::Chars
};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePosition
{
    pub line: usize,
    pub char: usize
}

impl CodePosition
{
    pub fn new() -> Self
    {
        Self{line: 1, char: 1}
    }

    pub fn next_char(&mut self)
    {
        self.char += 1;
    }

    pub fn next_line(&mut self)
    {
        self.line += 1;
        self.char = 1;
    }
}

impl Display for CodePosition
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}:{}", self.line, self.char)
    }
}

#[derive(Debug)]
pub struct LexemePos
{
    pub position: CodePosition,
    pub lexeme: Lexeme
}

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
    pub fn parse(text: &'a str) -> Vec<LexemePos>
    {
        let this = Self{
            position: CodePosition::new(),
            chars: text.chars(),
            current_char: None
        };

        this.parse_lexemes()
    }

    // wow not returning an iterator? so inefficient wow wow wow
    fn parse_lexemes(mut self) -> Vec<LexemePos>
    {
        iter::from_fn(|| self.parse_one()).collect()
    }

    fn parse_one(&mut self) -> Option<LexemePos>
    {
        let mut position = self.position;
        let mut current = String::new();
        let mut comment = false;
        
        loop
        {
            if let Some(c) = self.next_char()
            {
                self.position.next_char();

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

                if comment || c == ';'
                {
                    if current.is_empty()
                    {
                        self.consume_char();
                        comment = true;
                        continue;
                    }

                    return Some(LexemePos{
                        position,
                        lexeme: Lexeme::Value(current)
                    });
                }

                if c.is_whitespace()
                {
                    self.consume_char();

                    if current.is_empty()
                    {
                        position = self.position;
                        continue;
                    }

                    return Some(LexemePos{
                        position,
                        lexeme: Lexeme::Value(current)
                    });
                }

                if c == '\''
                {
                    if current.is_empty()
                    {
                        self.consume_char();

                        return Some(LexemePos{
                            position,
                            lexeme: Lexeme::Quote
                        });
                    }

                    return Some(LexemePos{
                        position,
                        lexeme: Lexeme::Value(current)
                    });
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

                        return Some(LexemePos{
                            position,
                            lexeme
                        });
                    }

                    return Some(LexemePos{
                        position,
                        lexeme: Lexeme::Value(current)
                    });
                }

                current.push(c);
                self.consume_char();
            } else
            {
                if current.is_empty()
                {
                    return None;
                }

                return Some(LexemePos{
                    position: self.position,
                    lexeme: Lexeme::Value(current)
                });
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
