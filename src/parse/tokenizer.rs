#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    StringLit(String),
    NumberLit(f64),
    BoolLit(bool),
    Ident(String),

    // Keywords
    Contract,
    Struct,
    Satisfies,
    Fn,
    Pub,
    Const,
    Let,
    Return,
    If,
    Else,
    For,
    In,
    Match,

    // Blocks
    Crash,
    Test,
    Mock,

    // Error
    Err,
    Ok,

    // Crash strategies
    Retry,
    Skip,
    Halt,
    Fallback,
    Default,

    // Import
    Import,
    From,
    Std,
    ColonColon, // ::

    // Self
    SelfKw,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Assign,    // =
    Eq,        // ==
    Neq,       // !=
    Lt,        // <
    Gt,        // >
    Lte,       // <=
    Gte,       // >=
    And,       // &&
    Or,        // ||
    Not,       // !
    Arrow,     // ->

    // Punctuation
    Dot,
    Comma,
    Colon,
    Semicolon,

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    // Special
    FatArrow,  // =>
    Is,        // is keyword for test assertions

    EOF,
}

/// Token with source location
#[derive(Debug, Clone)]
pub struct Located {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

pub fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Skip whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Skip comments
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // String literals
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '\'' => s.push('\''),
                        other => {
                            s.push('\\');
                            s.push(other);
                        }
                    }
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            i += 1; // closing quote
            tokens.push(Token::StringLit(s));
            continue;
        }

        // Numbers
        if c.is_ascii_digit() {
            let mut num = String::new();
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                // Only consume dot if next char is a digit (not a method call)
                if chars[i] == '.' {
                    if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                        num.push(chars[i]);
                    } else {
                        break;
                    }
                } else {
                    num.push(chars[i]);
                }
                i += 1;
            }
            tokens.push(Token::NumberLit(num.parse().unwrap()));
            continue;
        }

        // Identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let mut ident = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
            }
            tokens.push(match ident.as_str() {
                "contract" => Token::Contract,
                "struct" => Token::Struct,
                "satisfies" => Token::Satisfies,
                "fn" => Token::Fn,
                "pub" => Token::Pub,
                "const" => Token::Const,
                "let" => Token::Let,
                "return" => Token::Return,
                "if" => Token::If,
                "else" => Token::Else,
                "for" => Token::For,
                "in" => Token::In,
                "match" => Token::Match,
                "crash" => Token::Crash,
                "test" => Token::Test,
                "mock" => Token::Mock,
                "err" => Token::Err,
                "Ok" => Token::Ok,
                "retry" => Token::Retry,
                "skip" => Token::Skip,
                "halt" => Token::Halt,
                "fallback" => Token::Fallback,
                "default" => Token::Default,
                "import" => Token::Import,
                "from" => Token::From,
                "std" => Token::Std,
                "log" => Token::Ident("log".to_string()),
                "self" => Token::SelfKw,
                "is" => Token::Is,
                "true" => Token::BoolLit(true),
                "false" => Token::BoolLit(false),
                _ => Token::Ident(ident),
            });
            continue;
        }

        // Two-character operators
        if i + 1 < chars.len() {
            let two = format!("{}{}", c, chars[i + 1]);
            match two.as_str() {
                "::" => { tokens.push(Token::ColonColon); i += 2; continue; }
                "->" => { tokens.push(Token::Arrow); i += 2; continue; }
                "=>" => { tokens.push(Token::FatArrow); i += 2; continue; }
                "==" => { tokens.push(Token::Eq); i += 2; continue; }
                "!=" => { tokens.push(Token::Neq); i += 2; continue; }
                "<=" => { tokens.push(Token::Lte); i += 2; continue; }
                ">=" => { tokens.push(Token::Gte); i += 2; continue; }
                "&&" => { tokens.push(Token::And); i += 2; continue; }
                "||" => { tokens.push(Token::Or); i += 2; continue; }
                _ => {}
            }
        }

        // Single-character tokens
        match c {
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '=' => tokens.push(Token::Assign),
            '<' => tokens.push(Token::Lt),
            '>' => tokens.push(Token::Gt),
            '!' => tokens.push(Token::Not),
            '.' => tokens.push(Token::Dot),
            ',' => tokens.push(Token::Comma),
            ':' => tokens.push(Token::Colon),
            ';' => tokens.push(Token::Semicolon),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
            '[' => tokens.push(Token::LBracket),
            ']' => tokens.push(Token::RBracket),
            _ => panic!("unexpected character: '{}'", c),
        }
        i += 1;
    }

    tokens.push(Token::EOF);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_contract() {
        let tokens = tokenize("contract Stringable { to_string() -> String }");
        assert_eq!(tokens[0], Token::Contract);
        assert_eq!(tokens[1], Token::Ident("Stringable".into()));
        assert_eq!(tokens[2], Token::LBrace);
        assert_eq!(tokens[3], Token::Ident("to_string".into()));
        assert_eq!(tokens[4], Token::LParen);
        assert_eq!(tokens[5], Token::RParen);
        assert_eq!(tokens[6], Token::Arrow);
        assert_eq!(tokens[7], Token::Ident("String".into()));
        assert_eq!(tokens[8], Token::RBrace);
    }

    #[test]
    fn tokenize_struct() {
        let tokens = tokenize("pub struct Email { value: String }");
        assert_eq!(tokens[0], Token::Pub);
        assert_eq!(tokens[1], Token::Struct);
        assert_eq!(tokens[2], Token::Ident("Email".into()));
    }

    #[test]
    fn tokenize_crash_block() {
        // crash { http . get -> retry ( 3 , 1000 ) }
        // 0     1 2    3 4   5  6     7 8 9 10  11 12
        let tokens = tokenize("crash { http.get -> retry(3, 1000) }");
        assert_eq!(tokens[0], Token::Crash);
        assert_eq!(tokens[1], Token::LBrace);
        assert_eq!(tokens[5], Token::Arrow);
        assert_eq!(tokens[6], Token::Retry);
    }

    #[test]
    fn tokenize_test_block() {
        // test { self ( 1 , 2 ) == 3 }
        // 0    1 2    3 4 5 6 7 8  9 10
        let tokens = tokenize("test { self(1, 2) == 3 }");
        assert_eq!(tokens[0], Token::Test);
        assert_eq!(tokens[2], Token::SelfKw);
        assert_eq!(tokens[8], Token::Eq);
    }

    #[test]
    fn tokenize_err_ref() {
        let tokens = tokenize("err.timeout");
        assert_eq!(tokens[0], Token::Err);
        assert_eq!(tokens[1], Token::Dot);
        assert_eq!(tokens[2], Token::Ident("timeout".into()));
    }

    #[test]
    fn tokenize_err_decl() {
        let tokens = tokenize("err timeout = \"request timed out\"");
        assert_eq!(tokens[0], Token::Err);
        assert_eq!(tokens[1], Token::Ident("timeout".into()));
        assert_eq!(tokens[2], Token::Assign);
        assert_eq!(tokens[3], Token::StringLit("request timed out".into()));
    }

    #[test]
    fn tokenize_satisfies() {
        let tokens = tokenize("Email satisfies Stringable {");
        assert_eq!(tokens[0], Token::Ident("Email".into()));
        assert_eq!(tokens[1], Token::Satisfies);
        assert_eq!(tokens[2], Token::Ident("Stringable".into()));
    }

    #[test]
    fn tokenize_function() {
        // pub fn greet ( name : String ) -> String , err {
        // 0   1  2     3 4    5 6      7 8  9      10 11 12
        let tokens = tokenize("pub fn greet(name: String) -> String, err {");
        assert_eq!(tokens[0], Token::Pub);
        assert_eq!(tokens[1], Token::Fn);
        assert_eq!(tokens[2], Token::Ident("greet".into()));
        assert_eq!(tokens[10], Token::Comma);
        assert_eq!(tokens[11], Token::Err);
    }

    #[test]
    fn tokenize_is_keyword() {
        // self ( "bad" ) is err . invalid
        // 0    1 2      3 4  5   6 7
        let tokens = tokenize("self(\"bad\") is err.invalid");
        assert_eq!(tokens[0], Token::SelfKw);
        assert_eq!(tokens[4], Token::Is);
        assert_eq!(tokens[5], Token::Err);
    }

    #[test]
    fn tokenize_string_escapes() {
        let tokens = tokenize(r#""hello\nworld""#);
        assert_eq!(tokens[0], Token::StringLit("hello\nworld".into()));
    }

    #[test]
    fn tokenize_float() {
        let tokens = tokenize("3.14");
        assert_eq!(tokens[0], Token::NumberLit(3.14));
    }

    #[test]
    fn tokenize_method_on_number() {
        let tokens = tokenize("42.to_string()");
        assert_eq!(tokens[0], Token::NumberLit(42.0));
        assert_eq!(tokens[1], Token::Dot);
        assert_eq!(tokens[2], Token::Ident("to_string".into()));
    }
}
