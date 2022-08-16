#[cfg(test)]
mod parser_test;

#[macro_use]
mod macros;
pub mod ast;

use bitflags::bitflags;

use crate::error::{self, VestiErr, VestiParseErrKind};
use crate::lexer::token::Token;
use crate::lexer::token::TokenType;
use crate::lexer::Lexer;
use crate::location::Span;
use ast::*;

const ENV_MATH_IDENT: [&str; 5] = ["equation", "align", "array", "eqnarray", "gather"];

bitflags! {
    struct DocState: u8 {
        const DOC_START = 1 << 0;
        const PREVENT_END_DOC = 1 << 1;
        const PARSING_FUNCTION_DEF = 1 << 2;
    }
}

impl DocState {
    fn new() -> Self {
        Self { bits: 0 }
    }
}

pub struct Parser<'a> {
    source: Lexer<'a>,
    peek_tok: Token,
    document_state: DocState,
}

impl<'a> Parser<'a> {
    // Store Parser in the heap
    pub fn new(source: Lexer<'a>) -> Box<Self> {
        let mut output = Box::new(Self {
            source,
            peek_tok: Token::default(),
            document_state: DocState::new(),
        });
        output.next_tok();

        output
    }

    fn next_tok(&mut self) -> Token {
        let curr_tok = std::mem::take(&mut self.peek_tok);
        self.peek_tok = self.source.next();

        curr_tok
    }

    #[inline]
    fn peek_tok(&mut self) -> TokenType {
        self.peek_tok.toktype
    }

    #[inline]
    fn peek_tok_location(&mut self) -> Span {
        self.peek_tok.span
    }

    #[inline]
    fn is_eof(&self) -> bool {
        self.peek_tok.toktype == TokenType::Eof
    }

    fn eat_whitespaces(&mut self, newline_handle: bool) {
        while self.peek_tok() == TokenType::Space
            || self.peek_tok() == TokenType::Tab
            || (newline_handle && self.peek_tok() == TokenType::Newline)
        {
            self.next_tok();
        }
    }

    pub fn parse_latex(&mut self) -> error::Result<Latex> {
        let mut latex: Latex = Vec::new();
        while !self.is_eof() {
            latex.push(self.parse_statement()?);
        }
        if self.document_state == DocState::DOC_START {
            latex.push(Statement::DocumentEnd);
        }

        Ok(latex)
    }

    fn parse_statement(&mut self) -> error::Result<Statement> {
        let is_premiere =
            (self.document_state & (DocState::DOC_START | DocState::PARSING_FUNCTION_DEF)).bits();
        let is_function_def_state = (self.document_state & DocState::PARSING_FUNCTION_DEF).bits();

        match self.peek_tok() {
            // Keywords
            TokenType::Docclass if is_premiere == 0 => self.parse_docclass(),
            TokenType::Import if is_premiere == 0 => self.parse_usepackage(),
            TokenType::StartDoc if is_premiere == 0 => {
                self.document_state |= DocState::DOC_START;
                self.next_tok();
                self.eat_whitespaces(true);
                Ok(Statement::DocumentStart)
            }
            TokenType::Begenv => self.parse_environment(),
            TokenType::Endenv => Err(VestiErr::make_parse_err(
                VestiParseErrKind::IsNotOpenedErr {
                    open: vec![
                        TokenType::Begenv,
                        TokenType::Defenv,
                        TokenType::Redefenv,
                        TokenType::EndsWith,
                    ],
                    close: TokenType::Endenv,
                },
                self.peek_tok_location(),
            )),
            TokenType::Mtxt => self.parse_text_in_math(),
            TokenType::Etxt => Err(VestiErr::make_parse_err(
                VestiParseErrKind::IsNotOpenedErr {
                    open: vec![TokenType::Mtxt],
                    close: TokenType::Etxt,
                },
                self.peek_tok_location(),
            )),
            TokenType::DocumentStartMode => {
                self.document_state |= DocState::PREVENT_END_DOC | DocState::DOC_START;
                let loc = self.next_tok().span;
                expect_peek!(self: TokenType::Newline; loc);
                self.parse_statement()
            }
            TokenType::FunctionDef
            | TokenType::LongFunctionDef
            | TokenType::OuterFunctionDef
            | TokenType::LongOuterFunctionDef
            | TokenType::EFunctionDef
            | TokenType::LongEFunctionDef
            | TokenType::OuterEFunctionDef
            | TokenType::LongOuterEFunctionDef
            | TokenType::GFunctionDef
            | TokenType::LongGFunctionDef
            | TokenType::OuterGFunctionDef
            | TokenType::LongOuterGFunctionDef
            | TokenType::XFunctionDef
            | TokenType::LongXFunctionDef
            | TokenType::OuterXFunctionDef
            | TokenType::LongOuterXFunctionDef => self.parse_function_definition(),
            TokenType::EndFunctionDef => Err(VestiErr::make_parse_err(
                VestiParseErrKind::IsNotOpenedErr {
                    open: TokenType::get_function_definition_start_list(),
                    close: TokenType::EndFunctionDef,
                },
                self.peek_tok_location(),
            )),
            TokenType::Defenv | TokenType::Redefenv => self.parse_environment_definition(),
            TokenType::EndsWith => Err(VestiErr::make_parse_err(
                VestiParseErrKind::IsNotOpenedErr {
                    open: vec![TokenType::Defenv, TokenType::Redefenv],
                    close: TokenType::EndsWith,
                },
                self.peek_tok_location(),
            )),

            // Identifiers
            TokenType::LatexFunction => self.parse_latex_function(),
            TokenType::RawLatex => self.parse_raw_latex(),
            TokenType::Integer => self.parse_integer(),
            TokenType::Float => self.parse_float(),
            toktype if toktype.should_not_use_before_doc() && is_premiere == 0 => {
                Err(VestiErr::make_parse_err(
                    VestiParseErrKind::BeforeDocumentErr { got: toktype },
                    self.peek_tok_location(),
                ))
            }

            // Math related tokens
            TokenType::TextMathStart | TokenType::InlineMathStart => self.parse_math_stmt(),
            TokenType::Superscript | TokenType::Subscript
                if !self.source.math_started && is_function_def_state == 0 =>
            {
                Err(VestiErr::make_parse_err(
                    VestiParseErrKind::IllegalUseErr {
                        got: self.peek_tok(),
                    },
                    self.peek_tok_location(),
                ))
            }

            TokenType::TextMathEnd => Err(VestiErr::make_parse_err(
                VestiParseErrKind::InvalidTokToConvert {
                    got: TokenType::TextMathEnd,
                },
                self.peek_tok_location(),
            )),
            TokenType::InlineMathEnd => Err(VestiErr::make_parse_err(
                VestiParseErrKind::InvalidTokToConvert {
                    got: TokenType::InlineMathEnd,
                },
                self.peek_tok_location(),
            )),

            TokenType::Illegal => Err(VestiErr::make_parse_err(
                VestiParseErrKind::IllegalCharacterFoundErr,
                self.peek_tok_location(),
            )),

            _ => self.parse_main_stmt(),
        }
    }

    fn parse_integer(&mut self) -> error::Result<Statement> {
        let curr_tok = self.next_tok();
        let output = if let Ok(int) = curr_tok.literal.parse() {
            int
        } else {
            return Err(VestiErr::make_parse_err(
                VestiParseErrKind::ParseIntErr,
                curr_tok.span,
            ));
        };

        Ok(Statement::Integer(output))
    }

    fn parse_float(&mut self) -> error::Result<Statement> {
        let curr_tok = self.next_tok();
        let output = if let Ok(float) = curr_tok.literal.parse() {
            float
        } else {
            return Err(VestiErr::make_parse_err(
                VestiParseErrKind::ParseFloatErr,
                curr_tok.span,
            ));
        };

        Ok(Statement::Float(output))
    }

    fn parse_raw_latex(&mut self) -> error::Result<Statement> {
        Ok(Statement::RawLatex(self.next_tok().literal))
    }

    fn parse_main_stmt(&mut self) -> error::Result<Statement> {
        if self.is_eof() {
            return Err(VestiErr::make_parse_err(
                VestiParseErrKind::EOFErr,
                self.peek_tok_location(),
            ));
        }
        let text = self.next_tok().literal;

        Ok(Statement::MainText(text))
    }

    fn parse_math_stmt(&mut self) -> error::Result<Statement> {
        let start_location = self.peek_tok_location();
        let mut text = Vec::new();
        let mut stmt;

        match self.peek_tok() {
            TokenType::TextMathStart => {
                expect_peek!(self: TokenType::TextMathStart; self.peek_tok_location());

                while self.peek_tok() != TokenType::TextMathEnd {
                    stmt = match self.parse_statement() {
                        Ok(stmt) => stmt,
                        Err(VestiErr::ParseErr {
                            err_kind: VestiParseErrKind::EOFErr,
                            ..
                        }) => {
                            return Err(VestiErr::make_parse_err(
                                VestiParseErrKind::BracketMismatchErr {
                                    expected: TokenType::TextMathEnd,
                                },
                                start_location,
                            ));
                        }
                        Err(err) => return Err(err),
                    };
                    text.push(stmt);
                }

                expect_peek!(self: TokenType::TextMathEnd; self.peek_tok_location());
                Ok(Statement::MathText {
                    state: MathState::Text,
                    text,
                })
            }

            TokenType::InlineMathStart => {
                expect_peek!(self: TokenType::InlineMathStart; self.peek_tok_location());

                while self.peek_tok() != TokenType::InlineMathEnd {
                    stmt = match self.parse_statement() {
                        Ok(stmt) => stmt,
                        Err(VestiErr::ParseErr {
                            err_kind: VestiParseErrKind::EOFErr,
                            ..
                        }) => {
                            return Err(VestiErr::make_parse_err(
                                VestiParseErrKind::BracketMismatchErr {
                                    expected: TokenType::TextMathEnd,
                                },
                                start_location,
                            ));
                        }
                        Err(err) => return Err(err),
                    };
                    text.push(stmt);
                }

                expect_peek!(self: TokenType::InlineMathEnd; self.peek_tok_location());
                Ok(Statement::MathText {
                    state: MathState::Inline,
                    text,
                })
            }

            TokenType::Eof => Err(VestiErr::make_parse_err(
                VestiParseErrKind::EOFErr,
                self.peek_tok_location(),
            )),

            toktype => Err(VestiErr::make_parse_err(
                VestiParseErrKind::TypeMismatch {
                    expected: vec![TokenType::TextMathStart, TokenType::InlineMathStart],
                    got: toktype,
                },
                self.peek_tok_location(),
            )),
        }
    }

    fn parse_text_in_math(&mut self) -> error::Result<Statement> {
        let mut output: Latex = Vec::new();

        expect_peek!(self: TokenType::Mtxt; self.peek_tok_location());
        self.eat_whitespaces(false);

        while self.peek_tok() != TokenType::Etxt {
            if self.is_eof() {
                return Err(VestiErr::make_parse_err(
                    VestiParseErrKind::BracketMismatchErr {
                        expected: TokenType::Etxt,
                    },
                    self.peek_tok_location(),
                ));
            }
            output.push(self.parse_statement()?);
        }

        expect_peek!(self: TokenType::Etxt; self.peek_tok_location());

        Ok(Statement::PlainTextInMath(output))
    }

    fn parse_docclass(&mut self) -> error::Result<Statement> {
        let mut options: Option<Vec<Latex>> = None;

        expect_peek!(self: TokenType::Docclass; self.peek_tok_location());
        self.eat_whitespaces(false);

        take_name!(let name: String <- self);

        self.parse_comma_args(&mut options)?;
        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }

        Ok(Statement::DocumentClass { name, options })
    }

    fn parse_usepackage(&mut self) -> error::Result<Statement> {
        expect_peek!(self: TokenType::Import; self.peek_tok_location());
        self.eat_whitespaces(false);

        if self.peek_tok() == TokenType::Lbrace {
            return self.parse_multiple_usepackages();
        }

        let mut options: Option<Vec<Latex>> = None;
        take_name!(let name: String <- self);

        self.parse_comma_args(&mut options)?;
        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }

        Ok(Statement::Usepackage { name, options })
    }

    fn parse_multiple_usepackages(&mut self) -> error::Result<Statement> {
        let mut pkgs: Vec<Statement> = Vec::new();

        expect_peek!(self: TokenType::Lbrace; self.peek_tok_location());
        self.eat_whitespaces(true);

        while self.peek_tok() != TokenType::Rbrace {
            let mut options: Option<Vec<Latex>> = None;
            take_name!(let name: String <- self);

            self.parse_comma_args(&mut options)?;

            match self.peek_tok() {
                TokenType::Newline => self.eat_whitespaces(true),
                TokenType::Text | TokenType::RawLatex => {}
                TokenType::Rbrace => {
                    pkgs.push(Statement::Usepackage { name, options });
                    break;
                }
                TokenType::Eof => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::EOFErr,
                        self.peek_tok_location(),
                    ));
                }
                tok_type => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::TypeMismatch {
                            expected: vec![
                                TokenType::Newline,
                                TokenType::Rbrace,
                                TokenType::Text,
                                TokenType::RawLatex,
                            ],
                            got: tok_type,
                        },
                        self.peek_tok_location(),
                    ));
                }
            }

            pkgs.push(Statement::Usepackage { name, options });
        }

        expect_peek!(self: TokenType::Rbrace; self.peek_tok_location());

        self.eat_whitespaces(false);
        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }

        Ok(Statement::MultiUsepackages { pkgs })
    }

    fn parse_environment(&mut self) -> error::Result<Statement> {
        let begenv_location = self.peek_tok_location();
        let mut off_math_state = false;

        expect_peek!(self: TokenType::Begenv; self.peek_tok_location());
        self.eat_whitespaces(false);

        if self.is_eof() {
            return Err(VestiErr::ParseErr {
                err_kind: VestiParseErrKind::IsNotClosedErr {
                    open: TokenType::Begenv,
                    close: TokenType::Endenv,
                },
                location: begenv_location,
            });
        }

        let mut name = match self.peek_tok() {
            TokenType::Text => self.next_tok().literal,
            TokenType::Eof => {
                return Err(VestiErr::make_parse_err(
                    VestiParseErrKind::EOFErr,
                    begenv_location,
                ))
            }
            _ => {
                return Err(VestiErr::make_parse_err(
                    VestiParseErrKind::NameMissErr {
                        r#type: TokenType::Begenv,
                    },
                    begenv_location,
                ))
            }
        };

        // If name is math related one, then math mode will be turn on
        if ENV_MATH_IDENT.contains(&name.as_str()) {
            self.source.math_started = true;
            off_math_state = true;
        }

        while self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            name.push('*');
        }
        self.eat_whitespaces(false);

        let args = self.parse_function_args(
            TokenType::Lparen,
            TokenType::Rparen,
            TokenType::Lsqbrace,
            TokenType::Rsqbrace,
        )?;
        let mut text: Latex = Vec::new();

        while self.peek_tok() != TokenType::Endenv {
            if self.is_eof() {
                return Err(VestiErr::make_parse_err(
                    VestiParseErrKind::IsNotClosedErr {
                        open: TokenType::Begenv,
                        close: TokenType::Endenv,
                    },
                    begenv_location,
                ));
            }
            text.push(self.parse_statement()?);
        }

        expect_peek!(self: TokenType::Endenv; self.peek_tok_location());

        // If name is math related one, then math mode will be turn off
        if off_math_state {
            self.source.math_started = false;
        }
        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }

        Ok(Statement::Environment { name, args, text })
    }

    fn parse_function_definition(&mut self) -> error::Result<Statement> {
        let begfntdef_location = self.peek_tok_location();
        let mut trim = TrimWhitespace {
            start: true,
            mid: None,
            end: true,
        };

        let (style, beg_toktype) = match self.peek_tok() {
            TokenType::Eof => {
                return Err(VestiErr::ParseErr {
                    err_kind: VestiParseErrKind::EOFErr,
                    location: begfntdef_location,
                })
            }
            toktype => match toktype.try_into().map(|style| (style, toktype)) {
                Ok(tup) => tup,
                Err(got) => {
                    return Err(VestiErr::ParseErr {
                        err_kind: VestiParseErrKind::TypeMismatch {
                            expected: TokenType::get_function_definition_start_list(),
                            got,
                        },
                        location: begfntdef_location,
                    })
                }
            },
        };
        expect_peek!(self: beg_toktype; self.peek_tok_location());

        if self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            trim.start = false;
        }
        self.eat_whitespaces(false);

        if self.is_eof() {
            return Err(VestiErr::ParseErr {
                err_kind: VestiParseErrKind::IsNotClosedErr {
                    open: beg_toktype,
                    close: TokenType::EndFunctionDef,
                },
                location: begfntdef_location,
            });
        }

        let mut name = String::new();
        loop {
            name.push_str(
                match self.peek_tok() {
                    TokenType::Text | TokenType::ArgSpliter => self.next_tok().literal,
                    TokenType::Space | TokenType::Tab | TokenType::Newline | TokenType::Lparen => {
                        break
                    }
                    TokenType::Eof => {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::EOFErr,
                            begfntdef_location,
                        ));
                    }
                    _ => {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::NameMissErr {
                                r#type: beg_toktype,
                            },
                            begfntdef_location,
                        ));
                    }
                }
                .as_str(),
            );
        }
        self.eat_whitespaces(false);

        let args = self.parse_function_definition_argument()?;

        let body =
            self.parse_define_body(beg_toktype, TokenType::EndFunctionDef, begfntdef_location)?;
        expect_peek!(self: TokenType::EndFunctionDef; self.peek_tok_location());

        if self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            trim.end = false;
        }

        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }
        Ok(Statement::FunctionDefine {
            style,
            name,
            args,
            trim,
            body,
        })
    }

    fn parse_environment_definition(&mut self) -> error::Result<Statement> {
        let begenvdef_location = self.peek_tok_location();
        let mut trim = TrimWhitespace {
            start: true,
            mid: Some(true),
            end: true,
        };

        let (is_redefine, beg_toktype) = match self.peek_tok() {
            TokenType::Eof => {
                return Err(VestiErr::ParseErr {
                    err_kind: VestiParseErrKind::EOFErr,
                    location: begenvdef_location,
                })
            }
            TokenType::Defenv => (false, TokenType::Defenv),
            TokenType::Redefenv => (true, TokenType::Redefenv),
            got => {
                return Err(VestiErr::ParseErr {
                    err_kind: VestiParseErrKind::TypeMismatch {
                        expected: TokenType::get_function_definition_start_list(),
                        got,
                    },
                    location: begenvdef_location,
                })
            }
        };
        expect_peek!(self: beg_toktype; self.peek_tok_location());

        if self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            trim.start = false;
        }
        self.eat_whitespaces(false);

        if self.is_eof() {
            return Err(VestiErr::ParseErr {
                err_kind: VestiParseErrKind::IsNotClosedErr {
                    open: beg_toktype,
                    close: TokenType::EndsWith,
                },
                location: begenvdef_location,
            });
        }

        let mut name = String::new();
        loop {
            name.push_str(
                match self.peek_tok() {
                    TokenType::Text | TokenType::ArgSpliter => self.next_tok().literal,
                    TokenType::Space
                    | TokenType::Tab
                    | TokenType::Newline
                    | TokenType::Lsqbrace => break,
                    TokenType::Eof => {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::EOFErr,
                            begenvdef_location,
                        ));
                    }
                    _ => {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::NameMissErr {
                                r#type: beg_toktype,
                            },
                            begenvdef_location,
                        ));
                    }
                }
                .as_str(),
            );
        }
        self.eat_whitespaces(false);

        let (args_num, optional_arg) = if self.peek_tok() == TokenType::Lsqbrace {
            expect_peek!(self: TokenType::Lsqbrace; self.peek_tok_location());
            let tmp_argnum = match self.peek_tok() {
                TokenType::Integer => {
                    let lex_token = self.next_tok();
                    if let Ok(num) = lex_token.literal.parse::<u8>() {
                        num
                    } else {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::ParseIntErr,
                            lex_token.span,
                        ));
                    }
                }
                TokenType::Eof => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::EOFErr,
                        begenvdef_location,
                    ));
                }
                got => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::TypeMismatch {
                            expected: vec![TokenType::Integer],
                            got,
                        },
                        self.peek_tok_location(),
                    ))
                }
            };

            match self.peek_tok() {
                TokenType::Comma => {
                    expect_peek!(self: TokenType::Comma; self.peek_tok_location());
                    if self.peek_tok() == TokenType::Space {
                        self.next_tok();
                    }
                    let mut tmp_inner: Latex = Vec::new();
                    while self.peek_tok() != TokenType::Rsqbrace {
                        if self.is_eof() {
                            return Err(VestiErr::make_parse_err(
                                VestiParseErrKind::EOFErr,
                                begenvdef_location,
                            ));
                        }
                        tmp_inner.push(self.parse_statement()?);
                    }
                    expect_peek!(self: TokenType::Rsqbrace; self.peek_tok_location());

                    (tmp_argnum, Some(tmp_inner))
                }
                TokenType::Rsqbrace => {
                    expect_peek!(self: TokenType::Rsqbrace; self.peek_tok_location());
                    (tmp_argnum, None)
                }
                TokenType::Eof => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::EOFErr,
                        begenvdef_location,
                    ));
                }
                got => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::TypeMismatch {
                            expected: vec![TokenType::Rsqbrace, TokenType::Comma],
                            got,
                        },
                        self.peek_tok_location(),
                    ))
                }
            }
        } else {
            (0, None)
        };

        let begin_part =
            self.parse_define_body(beg_toktype, TokenType::EndsWith, begenvdef_location)?;
        let midenvdef_location = self.peek_tok_location();
        expect_peek!(self: TokenType::EndsWith; self.peek_tok_location());
        // TODO: implement triming mid_left and mid_right separately.
        // In the present, there are only two states about triming in the middle.
        if self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            trim.mid = Some(false);
        }

        let end_part =
            self.parse_define_body(TokenType::EndsWith, TokenType::Endenv, midenvdef_location)?;
        expect_peek!(self: TokenType::Endenv; self.peek_tok_location());
        if self.peek_tok() == TokenType::Star {
            expect_peek!(self: TokenType::Star; self.peek_tok_location());
            trim.end = false;
        }
        if self.peek_tok() == TokenType::Newline {
            self.next_tok();
        }
        Ok(Statement::EnvironmentDefine {
            is_redefine,
            name,
            args_num,
            optional_arg,
            trim,
            begin_part,
            end_part,
        })
    }

    fn parse_latex_function(&mut self) -> error::Result<Statement> {
        let mut name = if self.is_eof() {
            return Err(VestiErr::ParseErr {
                err_kind: VestiParseErrKind::EOFErr,
                location: self.peek_tok_location(),
            });
        } else {
            self.next_tok().literal
        };

        let mut is_no_arg_but_space = false;
        if self.peek_tok() == TokenType::Space {
            is_no_arg_but_space = true;
            self.eat_whitespaces(false);
        }

        let args = self.parse_function_args(
            TokenType::Lbrace,
            TokenType::Rbrace,
            TokenType::Lsqbrace,
            TokenType::Rsqbrace,
        )?;
        if args.is_empty() && is_no_arg_but_space {
            name += " ";
        }

        Ok(Statement::LatexFunction { name, args })
    }

    fn parse_function_definition_argument(&mut self) -> error::Result<String> {
        let open_brace_location = self.peek_tok_location();
        let mut output = String::new();
        let mut parenthesis_level = 0;
        let mut is_first_token = true;

        expect_peek!(self: TokenType::Lparen; open_brace_location);
        while !self.is_eof() && parenthesis_level >= 0 {
            match self.peek_tok() {
                TokenType::Lparen => parenthesis_level += 1,
                TokenType::Rparen => {
                    parenthesis_level -= 1;
                    if parenthesis_level < 0 {
                        break;
                    }
                }
                TokenType::Eof => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::BracketNumberMatchedErr,
                        open_brace_location,
                    ))
                }
                _ => {}
            }

            if is_first_token && self.peek_tok() == TokenType::Text || self.peek_tok().is_keyword()
            {
                output.push(' ');
            }
            is_first_token = false;

            output.push_str(self.next_tok().literal.as_str());
        }
        expect_peek!(self: TokenType::Rparen; open_brace_location);

        Ok(output)
    }

    fn parse_define_body(
        &mut self,
        beg_toktype: TokenType,
        end_toktype: TokenType,
        begdef_location: Span,
    ) -> error::Result<Latex> {
        let mut body: Latex = Vec::new();
        let mut def_level = 0;
        while self.peek_tok() != end_toktype && def_level >= 0 {
            match self.peek_tok() {
                TokenType::Eof => {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::IsNotClosedErr {
                            open: beg_toktype,
                            close: end_toktype,
                        },
                        begdef_location,
                    ));
                }
                TokenType::FunctionDef
                | TokenType::LongFunctionDef
                | TokenType::OuterFunctionDef
                | TokenType::LongOuterFunctionDef
                | TokenType::EFunctionDef
                | TokenType::LongEFunctionDef
                | TokenType::OuterEFunctionDef
                | TokenType::LongOuterEFunctionDef
                | TokenType::GFunctionDef
                | TokenType::LongGFunctionDef
                | TokenType::OuterGFunctionDef
                | TokenType::LongOuterGFunctionDef
                | TokenType::XFunctionDef
                | TokenType::LongXFunctionDef
                | TokenType::OuterXFunctionDef
                | TokenType::LongOuterXFunctionDef => def_level += 1,
                toktype if toktype == end_toktype => {
                    def_level -= 1;
                    if def_level < 0 {
                        break;
                    }
                }
                _ => {}
            }
            self.document_state |= DocState::PARSING_FUNCTION_DEF;
            body.push(self.parse_statement()?);
            self.document_state &= !DocState::PARSING_FUNCTION_DEF;
        }

        Ok(body)
    }

    fn parse_comma_args(&mut self, options: &mut Option<Vec<Latex>>) -> error::Result<()> {
        self.eat_whitespaces(false);
        if self.peek_tok() == TokenType::Lparen {
            let mut options_vec: Vec<Latex> = Vec::new();
            // Since we yet tell to the computer to get the next token,
            // peeking the token location is the location of the open brace one.
            let open_brace_location = self.peek_tok_location();
            self.next_tok();
            self.eat_whitespaces(true);

            while self.peek_tok() != TokenType::Rparen {
                if self.is_eof() {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::BracketNumberMatchedErr,
                        open_brace_location,
                    ));
                }

                self.eat_whitespaces(true);
                let mut tmp: Latex = Vec::new();

                while self.peek_tok() != TokenType::Comma {
                    self.eat_whitespaces(true);
                    if self.is_eof() {
                        return Err(VestiErr::make_parse_err(
                            VestiParseErrKind::BracketNumberMatchedErr,
                            open_brace_location,
                        ));
                    }
                    if self.peek_tok() == TokenType::Rparen {
                        break;
                    }
                    tmp.push(self.parse_statement()?);
                }

                options_vec.push(tmp);
                self.eat_whitespaces(true);

                if self.peek_tok() == TokenType::Rparen {
                    break;
                }

                expect_peek!(self: TokenType::Comma; self.peek_tok_location());
                self.eat_whitespaces(true);
            }

            expect_peek!(self: TokenType::Rparen; self.peek_tok_location());
            self.eat_whitespaces(false);
            *options = Some(options_vec);
        }

        Ok(())
    }

    fn parse_function_args(
        &mut self,
        open: TokenType,
        closed: TokenType,
        optional_open: TokenType,
        optional_closed: TokenType,
    ) -> error::Result<Vec<(ArgNeed, Vec<Statement>)>> {
        let mut args: Vec<(ArgNeed, Vec<Statement>)> = Vec::new();

        if self.peek_tok() == open
            || self.peek_tok() == optional_open
            || self.peek_tok() == TokenType::Star
        {
            loop {
                match self.peek_tok() {
                    toktype if toktype == open => {
                        self.parse_function_args_core(&mut args, open, closed, ArgNeed::MainArg)?
                    }

                    toktype if toktype == optional_open => self.parse_function_args_core(
                        &mut args,
                        optional_open,
                        optional_closed,
                        ArgNeed::Optional,
                    )?,

                    TokenType::Star => {
                        expect_peek!(self: TokenType::Star; self.peek_tok_location());
                        args.push((ArgNeed::StarArg, Vec::new()));
                    }

                    _ => break,
                }

                if let TokenType::Eof | TokenType::Newline = self.peek_tok() {
                    break;
                }
            }
        }

        Ok(args)
    }

    fn parse_function_args_core(
        &mut self,
        args: &mut Vec<(ArgNeed, Vec<Statement>)>,
        open: TokenType,
        closed: TokenType,
        arg_need: ArgNeed,
    ) -> error::Result<()> {
        let open_brace_location = self.peek_tok_location();
        let mut nested = 0;
        expect_peek!(self: open; open_brace_location);

        loop {
            let mut tmp_vec: Vec<Statement> = Vec::new();
            while (self.peek_tok() != closed || nested > 0)
                && self.peek_tok() != TokenType::ArgSpliter
            {
                if self.is_eof() {
                    return Err(VestiErr::make_parse_err(
                        VestiParseErrKind::BracketNumberMatchedErr,
                        open_brace_location,
                    ));
                }
                if self.peek_tok() == open {
                    nested += 1;
                }
                if self.peek_tok() == closed {
                    nested -= 1;
                }
                let stmt = self.parse_statement()?;
                tmp_vec.push(stmt);
            }
            args.push((arg_need, tmp_vec));

            if self.peek_tok() != TokenType::ArgSpliter {
                break;
            }
            expect_peek!(self: TokenType::ArgSpliter; self.peek_tok_location());

            // Multiline splitting argument support
            self.eat_whitespaces(true);
        }
        expect_peek!(self: closed; self.peek_tok_location());

        Ok(())
    }
}
