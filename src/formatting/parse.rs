use nom::{
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_while, take_while1},
    character::complete::{anychar, char},
    combinator::{cut, eof, map, not, opt},
    multi::{many0, separated_list0},
    sequence::{preceded, separated_pair, terminated, tuple},
    IResult, Parser,
};

#[derive(Debug, PartialEq, Eq)]
pub struct Arg<'a> {
    pub key: &'a str,
    pub val: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Formatter<'a> {
    pub name: &'a str,
    pub args: Vec<Arg<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Placeholder<'a> {
    pub name: &'a str,
    pub formatter: Option<Formatter<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    Text(String),
    Placeholder(Placeholder<'a>),
    Icon(&'a str),
    Recursive(FormatTemplate<'a>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct TokenList<'a>(pub Vec<Token<'a>>);

#[derive(Debug, PartialEq, Eq)]
pub struct FormatTemplate<'a>(pub Vec<TokenList<'a>>);

fn spaces(i: &str) -> IResult<&str, &str> {
    take_while(|x: char| x.is_ascii_whitespace())(i)
}

fn alphanum1(i: &str) -> IResult<&str, &str> {
    take_while1(|x: char| x.is_alphanumeric() || x == '_' || x == '-')(i)
}

fn arg1(i: &str) -> IResult<&str, &str> {
    take_while1(|x: char| x.is_alphanumeric() || x == '_' || x == '-' || x == '.')(i)
}

// `key:val`
fn parse_arg(i: &str) -> IResult<&str, Arg> {
    map(
        separated_pair(alphanum1, cut(char(':')), cut(arg1)),
        |(key, val)| Arg { key, val },
    )(i)
}

// `(arg,key:val)`
// `( arg, key:val , abc)`
fn parse_args(i: &str) -> IResult<&str, Vec<Arg>> {
    let inner = separated_list0(preceded(spaces, char(',')), preceded(spaces, parse_arg));
    preceded(
        char('('),
        cut(terminated(inner, preceded(spaces, char(')')))),
    )(i)
}

// `.str(width:2)`
// `.eng(unit:bits,bin)`
fn parse_formatter(i: &str) -> IResult<&str, Formatter> {
    preceded(char('.'), cut(tuple((alphanum1, opt(parse_args)))))
        .map(|(name, args)| Formatter {
            name,
            args: args.unwrap_or_default(),
        })
        .parse(i)
}

// `$var`
// `$key.eng(unit:bits,bin)`
fn parse_placeholder(i: &str) -> IResult<&str, Placeholder> {
    preceded(char('$'), cut(tuple((alphanum1, opt(parse_formatter)))))
        .map(|(name, formatter)| Placeholder { name, formatter })
        .parse(i)
}

// `just escaped \| text`
fn parse_string(i: &str) -> IResult<&str, String> {
    preceded(
        not(eof),
        escaped_transform(
            take_while1(|x| x != '$' && x != '^' && x != '{' && x != '}' && x != '|' && x != '\\'),
            '\\',
            anychar,
        ),
    )(i)
}

// `^icon_name`
fn parse_icon(i: &str) -> IResult<&str, &str> {
    preceded(char('^'), cut(preceded(tag("icon_"), alphanum1)))(i)
}

// `{ a | b | c }`
fn parse_recursive_template(i: &str) -> IResult<&str, FormatTemplate> {
    preceded(char('{'), cut(terminated(parse_format_template, char('}'))))(i)
}

fn parse_token_list(i: &str) -> IResult<&str, TokenList> {
    map(
        many0(alt((
            map(parse_string, Token::Text),
            map(parse_placeholder, Token::Placeholder),
            map(parse_icon, Token::Icon),
            map(parse_recursive_template, Token::Recursive),
        ))),
        TokenList,
    )(i)
}

pub fn parse_format_template(i: &str) -> IResult<&str, FormatTemplate> {
    map(separated_list0(char('|'), parse_token_list), FormatTemplate)(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg() {
        assert_eq!(
            parse_arg("key:val,"),
            Ok((
                ",",
                Arg {
                    key: "key",
                    val: "val"
                }
            ))
        );
        assert!(parse_arg("key:,").is_err());
    }

    #[test]
    fn args() {
        assert_eq!(
            parse_args("(key:val)"),
            Ok((
                "",
                vec![Arg {
                    key: "key",
                    val: "val"
                }]
            ))
        );
        assert_eq!(
            parse_args("( abc:d , key:val )"),
            Ok((
                "",
                vec![
                    Arg {
                        key: "abc",
                        val: "d",
                    },
                    Arg {
                        key: "key",
                        val: "val"
                    }
                ]
            ))
        );
    }

    #[test]
    fn formatter() {
        assert_eq!(
            parse_formatter(".str(key:val)"),
            Ok((
                "",
                Formatter {
                    name: "str",
                    args: vec![Arg {
                        key: "key",
                        val: "val"
                    }]
                }
            ))
        );
        assert_eq!(
            parse_formatter(".eng(w:3 , bin:true )"),
            Ok((
                "",
                Formatter {
                    name: "eng",
                    args: vec![
                        Arg { key: "w", val: "3" },
                        Arg {
                            key: "bin",
                            val: "true"
                        }
                    ]
                }
            ))
        );
    }

    #[test]
    fn placeholder() {
        assert_eq!(
            parse_placeholder("$key"),
            Ok((
                "",
                Placeholder {
                    name: "key",
                    formatter: None,
                }
            ))
        );
        assert_eq!(
            parse_placeholder("$var.str()"),
            Ok((
                "",
                Placeholder {
                    name: "var",
                    formatter: Some(Formatter {
                        name: "str",
                        args: vec![]
                    }),
                }
            ))
        );
        assert_eq!(
            parse_placeholder("$var.str(a:b, c:d)"),
            Ok((
                "",
                Placeholder {
                    name: "var",
                    formatter: Some(Formatter {
                        name: "str",
                        args: vec![Arg { key: "a", val: "b" }, Arg { key: "c", val: "d" }]
                    }),
                }
            ))
        );
        assert!(parse_placeholder("$key.").is_err());
    }

    #[test]
    fn icon() {
        assert_eq!(parse_icon("^icon_my_icon"), Ok(("", "my_icon")));
        assert_eq!(parse_icon("^icon_m"), Ok(("", "m")));
        assert!(parse_icon("^icon_").is_err());
        assert!(parse_icon("^2").is_err());
    }

    #[test]
    fn token_list() {
        assert_eq!(
            parse_token_list(" abc \\$ $var.str(a:b)$x "),
            Ok((
                "",
                TokenList(vec![
                    Token::Text(" abc $ ".into()),
                    Token::Placeholder(Placeholder {
                        name: "var",
                        formatter: Some(Formatter {
                            name: "str",
                            args: vec![Arg { key: "a", val: "b" }]
                        })
                    }),
                    Token::Placeholder(Placeholder {
                        name: "x",
                        formatter: None,
                    }),
                    Token::Text(" ".into())
                ])
            ))
        );
    }

    #[test]
    fn format_template() {
        assert_eq!(
            parse_format_template("simple"),
            Ok((
                "",
                FormatTemplate(vec![TokenList(vec![Token::Text("simple".into())]),])
            ))
        );
        assert_eq!(
            parse_format_template(" $x.str() | N/A "),
            Ok((
                "",
                FormatTemplate(vec![
                    TokenList(vec![
                        Token::Text(" ".into()),
                        Token::Placeholder(Placeholder {
                            name: "x",
                            formatter: Some(Formatter {
                                name: "str",
                                args: vec![]
                            })
                        }),
                        Token::Text(" ".into()),
                    ]),
                    TokenList(vec![Token::Text(" N/A ".into())]),
                ])
            ))
        );
    }

    #[test]
    fn full() {
        assert_eq!(
            parse_format_template(" ^icon_my_icon {$x.str()|N/A} "),
            Ok((
                "",
                FormatTemplate(vec![TokenList(vec![
                    Token::Text(" ".into()),
                    Token::Icon("my_icon"),
                    Token::Text(" ".into()),
                    Token::Recursive(FormatTemplate(vec![
                        TokenList(vec![Token::Placeholder(Placeholder {
                            name: "x",
                            formatter: Some(Formatter {
                                name: "str",
                                args: vec![]
                            })
                        })]),
                        TokenList(vec![Token::Text("N/A".into())]),
                    ])),
                    Token::Text(" ".into()),
                ]),])
            ))
        );
    }
}
