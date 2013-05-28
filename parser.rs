use super::*;

///
/// A position in a source string
///
pub struct Position {
    line: uint,
    col: uint,
}
static DUMMY_POS: Position = Position { line: 0, col: 0 };

pub struct ParseFailure {
    description: ~str,
    position: Position,
}

struct Token<'self> {
    val: &'self str,
    position: Position,
}

impl<'self> Token<'self> {
    fn new<'r>(val: &'r str, position: Position) -> Token<'r> {
        Token { val: val, position: position }
    }
}


pub struct Parser<'self> {
    token_start: uint,
    position: uint,
    token: Option<Token<'self>>,
    src: &'self str
}

pub impl<'self> Parser<'self> {
    fn new<'r>(src: &'r str) -> Parser<'r> {
        Parser {
            token_start: 0,
            position: 0,
            token: None,
            src: src
        }
    }

    // Tokenising

    fn load_token(&mut self) {
        cond!(
            // already loaded
            (self.token.is_some()) { return; }
            // a left-over token
            (self.token_start < self.position) {
                self.token = Some(Token::new(self.src.slice(self.token_start, self.position),
                                             DUMMY_POS));
                self.token_start = self.position;
                return;
            }
            // finished
            (self.token_start >= self.src.len()) { self.token = None; return; }
        );

        let mut seen_token = false;
        for str::each_char(self.src.slice(self.position, self.src.len())) |c| {
            // TODO: tokenise strings
            match c {
                '(' | ')' => {
                    if !seen_token {
                        // we haven't seen a token yet, so use this character
                        self.token_start = self.position;
                        self.position += 1; // always 1 byte
                        seen_token = true;
                    } // we've already seen a token, so don't advance, and use that one
                    break;
                }
                // ignore (leading) whitespace
                _ if c.is_whitespace() => { if seen_token { break } }
                _ => {
                    if !seen_token { // start a new token here
                        self.token_start = self.position;
                        seen_token = true;
                    }
                }
            }
            self.position += char::len_utf8_bytes(c);
        }
        self.token = if seen_token {
            Some(Token::new(self.src.slice(self.token_start, self.position), DUMMY_POS))
        } else {
            None
        };
        self.token_start = self.position;
    }

    fn peek_token(&mut self) -> Option<Token<'self>> {
        // this is does nothing if token is Some
        self.load_token();
        self.token
    }

    fn peek_token_no_eof(&mut self) -> Result<Token<'self>, ParseFailure> {
        match self.peek_token() {
            Some(tok) => Ok(tok),
            None => Err(ParseFailure { position: DUMMY_POS, description: ~"Unexpected EOF"})
        }
    }

    fn bump_token(&mut self) {
        self.load_token();
        self.token = None;
    }

    fn eat_token(&mut self, tok: &str) -> bool {
        match self.peek_token() {
            Some(Token { val, _ }) if val == tok => {
                self.bump_token();
                true
            }
            _ => false,
        }
    }

    fn expect_token(&mut self, tok: &str) -> Result<(), ParseFailure> {
        if self.eat_token(tok) {
            Ok(())
        } else {
            Err(ParseFailure { position: DUMMY_POS, description: fmt!("Expecting %s", tok)})
        }
    }

    // Parsing

    /// Parse an identifier
    fn parse_ident(&mut self) -> Result<Ident, ParseFailure> {
        do self.peek_token_no_eof().chain |tok| {
            match tok.val {
                "" | "(" | ")" => {
                    Err(ParseFailure { position: tok.position, description: ~"empty ident" })
                }
                _ => {
                    self.bump_token();
                    Ok(tok.val.to_owned())
                }
            }
        }
    }

    fn parse_value_from_token(&mut self, tok: Token)  -> Result<Value, ParseFailure> {
        macro_rules! ret(($val:expr) => {{ self.bump_token(); return Ok($val); }});
        match tok.val {
            "true" => ret!(Bool(true)),
            "false" => ret!(Bool(false)),
            _ => {}
        }
        match int::from_str(tok.val) {
            Some(i) => ret!(Int(i)),
            None => {}
        }
        match float::from_str(tok.val) {
            Some(f) => ret!(Float(f)),
            None => {}
        }

        Err(ParseFailure { position: tok.position, description: ~"Invalid value"})
    }

    fn parse_value(&mut self) -> Result<Value, ParseFailure> {
        do self.peek_token_no_eof().chain |tok| { self.parse_value_from_token(tok) }
    }

    fn parse_value_or_ident(&mut self) -> Result<~Value, ParseFailure> {
        match self.parse_value() {
            Ok(val) => Ok(~val),
            Err(_) => do self.parse_ident().map |&ident| { ~Symbol(ident) }
        }
    }

    /// Parse an if expression, without the leading 'if'
    fn parse_list(&mut self) -> Result<~Value, ParseFailure> {
        Ok(~List(
            do vec::build |push| {
                loop {
                    match self.parse() {
                        Ok(val) => push(val),
                        Err(_) => break,
                    }
                }
            }
        ))
    }

    /// Parse an if expression, without the leading 'if'
    fn parse_if(&mut self) -> Result<~Value, ParseFailure> {
        do self.parse().chain |test| {
            do self.parse().chain |conseq| {
                do self.parse().map |&alt| {
                    ~If(test.clone(), conseq.clone(), alt)
                }
            }
        }
    }

    /// Parse a lambda expression, without the leading 'fn'
    fn parse_lambda(&mut self) -> Result<~Value, ParseFailure> {
        do self.expect_token("(").chain |_| {
            let args = do vec::build |push| {
                loop {
                    match self.parse_ident() {
                        Ok(ident) => push(ident),
                        Err(_) => break,
                    }
                }
            };

            match self.expect_token(")") {
                Err(err) => Err(err),
                _ => match self.parse() {
                    Ok(expr) => Ok(~Lambda(args, expr)),
                    Err(err) => Err(err)
                }
            }
        }
    }

    /// Parse a quote expression, without the leading 'quote'
    fn parse_quote(&mut self) -> Result<~Value, ParseFailure> {
        do self.parse().map |&expr| {
            ~Quote(expr)
        }
    }

    fn parse_def(&mut self) -> Result<~Value, ParseFailure> {
        do self.parse_ident().chain |ident| {
            do self.parse().map |&expr| {
                ~Def(copy ident, expr)
            }
        }
    }

    fn parse_do(&mut self) -> Result<~Value, ParseFailure> {
        let mut exprs = ~[];
        loop {
            match self.parse() {
                Ok(expr) => exprs.push(expr),
                Err(_) if exprs.len() >= 1 => break,
                Err(err) => return Err(err),
            }
        }
        Ok(~Do(exprs))
    }

    fn parse_apply(&mut self) -> Result<~Value, ParseFailure> {
        do self.parse().chain |expr| {
            let args = do vec::build |push| {
                loop {
                    match self.parse() {
                        Ok(expr) => push(expr),
                        Err(_) => break
                    }
                }
            };
            Ok(~Apply(expr, args))
        }
    }

    /// Parse the interior of an S-expr
    fn parse_parened(&mut self) -> Result<~Value, ParseFailure> {
        cond! (
            (self.eat_token("list"))  { self.parse_list() }
            (self.eat_token("if"))    { self.parse_if() }
            (self.eat_token("quote")) { self.parse_quote() }
            (self.eat_token("def"))   { self.parse_def() }
            (self.eat_token("fn"))    { self.parse_lambda() }
            (self.eat_token("do"))    { self.parse_do() }
            // this is a silly hack
            (self.peek_token().map_default(false,
                                           |x| x.val == ")")) { Ok(~Unit) }

            _ { self.parse_apply() }
        )
    }

    fn parse(&mut self) -> Result<~Value, ParseFailure> {
        if self.eat_token("(") {
            let ret = self.parse_parened();
            if !self.eat_token(")") {
                Err(ParseFailure { position: DUMMY_POS, description: ~"Expecting ')'"})
            } else {
                ret
            }
        } else {
            do self.peek_token_no_eof().chain |tok| {
                if (tok.val == ")") {
                    Err(ParseFailure { position: tok.position, description: ~"Unexpected ')'"})
                } else {
                    self.parse_value_or_ident()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_ok() {
        fn test(s: ~str) {
            assert_eq!(Parser::new(s).parse().get().to_str(), s);
        }

        test(~"1");
        test(~"(def a (+ 1 2))");
        test(~"(do (def a 1) (def b 2) (+ a b))");
        test(~"(list (1 2) 3 (+ 4 5))");

        // the extra space after the lambda?
        test(~"(if true (fn (a b) (+ 1 a b)) (quote (1 2 3)))");
    }

    #[test]
    fn test_parser_err() {
        fn test(s: ~str) {
             assert!(Parser::new(s).parse().is_err());
        }

        test(~"(");
        test(~")");

        test(~"(fn (a b (+ 1 a b)))");
        test(~"(if true)");
        test(~"(if true 1 2 3)");
        test(~"(def a)");
        test(~"(def a b c)");
        test(~"(do)");
    }
}