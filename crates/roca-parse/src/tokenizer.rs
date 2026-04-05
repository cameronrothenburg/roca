//! Tokenizer for the Roca language.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),

    // Identifiers and keywords
    Ident(String),

    // Keywords
    Pub,
    Fn,
    Let,
    Var,
    Const,
    Return,
    If,
    Else,
    For,
    In,
    Loop,
    Break,
    Continue,
    Match,
    Struct,
    Enum,
    Import,
    From,
    Wait,
    Self_,
    Unit,

    // Ownership qualifiers
    O, // owned
    B, // borrowed

    // Arrow
    Arrow,   // ->
    FatArrow, // =>

    // Punctuation
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Semicolon,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,      // ==
    Ne,      // !=
    Lt,      // <
    Gt,      // >
    Le,      // <=
    Ge,      // >=
    And,     // &&
    Or,      // ||
    Not,     // !
    Assign,  // =

    // Test keywords
    Test,

    // End of file
    Eof,
}

pub fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut pos = 0;
    let mut tokens = Vec::new();

    while pos < chars.len() {
        let ch = chars[pos];

        // Skip whitespace
        if ch.is_whitespace() {
            pos += 1;
            continue;
        }

        // Line comments
        if ch == '/' && pos + 1 < chars.len() && chars[pos + 1] == '/' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        // String literals
        if ch == '"' {
            pos += 1;
            let mut s = String::new();
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 1;
                    match chars[pos] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        c => { s.push('\\'); s.push(c); }
                    }
                } else {
                    s.push(chars[pos]);
                }
                pos += 1;
            }
            pos += 1; // closing quote
            tokens.push(Token::String(s));
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() {
            let start = pos;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos < chars.len() && chars[pos] == '.' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit() {
                pos += 1;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                }
                let s: String = chars[start..pos].iter().collect();
                tokens.push(Token::Float(s.parse().unwrap()));
            } else {
                let s: String = chars[start..pos].iter().collect();
                tokens.push(Token::Int(s.parse().unwrap()));
            }
            continue;
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let word: String = chars[start..pos].iter().collect();
            let tok = match word.as_str() {
                "pub" => Token::Pub,
                "fn" => Token::Fn,
                "let" => Token::Let,
                "var" => Token::Var,
                "const" => Token::Const,
                "return" => Token::Return,
                "if" => Token::If,
                "else" => Token::Else,
                "for" => Token::For,
                "in" => Token::In,
                "loop" => Token::Loop,
                "break" => Token::Break,
                "continue" => Token::Continue,
                "match" => Token::Match,
                "struct" => Token::Struct,
                "enum" => Token::Enum,
                "import" => Token::Import,
                "from" => Token::From,
                "wait" => Token::Wait,
                "self" => Token::Self_,
                "true" => Token::Bool(true),
                "false" => Token::Bool(false),
                "Unit" => Token::Unit,
                "test" => Token::Test,
                "o" => Token::O,
                "b" => Token::B,
                _ => Token::Ident(word),
            };
            tokens.push(tok);
            continue;
        }

        // Two-char operators
        if ch == '-' && pos + 1 < chars.len() && chars[pos + 1] == '>' {
            tokens.push(Token::Arrow);
            pos += 2;
            continue;
        }
        if ch == '=' && pos + 1 < chars.len() && chars[pos + 1] == '>' {
            tokens.push(Token::FatArrow);
            pos += 2;
            continue;
        }
        if ch == '=' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Eq);
            pos += 2;
            continue;
        }
        if ch == '!' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Ne);
            pos += 2;
            continue;
        }
        if ch == '<' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Le);
            pos += 2;
            continue;
        }
        if ch == '>' && pos + 1 < chars.len() && chars[pos + 1] == '=' {
            tokens.push(Token::Ge);
            pos += 2;
            continue;
        }
        if ch == '&' && pos + 1 < chars.len() && chars[pos + 1] == '&' {
            tokens.push(Token::And);
            pos += 2;
            continue;
        }
        if ch == '|' && pos + 1 < chars.len() && chars[pos + 1] == '|' {
            tokens.push(Token::Or);
            pos += 2;
            continue;
        }

        // Single-char tokens
        let tok = match ch {
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            ',' => Token::Comma,
            ':' => Token::Colon,
            '.' => Token::Dot,
            ';' => Token::Semicolon,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '<' => Token::Lt,
            '>' => Token::Gt,
            '!' => Token::Not,
            '=' => Token::Assign,
            _ => panic!("unexpected character: {:?} at position {}", ch, pos),
        };
        tokens.push(tok);
        pos += 1;
    }

    tokens.push(Token::Eof);
    tokens
}
