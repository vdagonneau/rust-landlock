use anyhow::Result;
use clap::{
    app_from_crate, crate_authors, crate_description, crate_name, crate_version, AppSettings, Arg,
};
use nom::combinator::{all_consuming, opt};
use nom::error::ParseError;
use nom::sequence::tuple;
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, multispace0, satisfy},
    combinator::{recognize, value},
    multi::{many0, separated_list0},
    sequence::{delimited, pair},
    IResult,
};
use std::path::PathBuf;

const ENV_FS_RO_NAME: &str = "LL_FS_RO";
const ENV_FS_RW_NAME: &str = "LL_FS_RW";

#[derive(Clone, Debug)]
enum EnvVar {
    RO,
    RW,
}

/// A combinator that takes a parser `inner` and produces a parser
/// that also consumes both leading and trailing whitespace, returning
/// the output of `inner`.
fn ws<'a, F: 'a, O, E: ParseError<&'a str>>(
    inner: F,
) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
where
    F: Fn(&'a str) -> IResult<&'a str, O, E>,
{
    delimited(multispace0, inner, multispace0)
}

pub fn peol_comment<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value(
        (), // Output is thrown away.
        pair(char('#'), is_not("\n\r")),
    )(input)
}

pub fn filename(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z')),
        many0(satisfy(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z'))),
    ))(input)
}

/// https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap03.html#tag_03_276
fn parse_path(input: &str) -> IResult<&str, PathBuf> {
    let path = PathBuf::from("/");
    let (input, _) = tag("/")(input)?;
    separated_list0(tag("/"), filename)(input)
        .map(|(i, vs)| (i, vs.iter().fold(path, |p, s| p.join(s))))
}

fn parse_path_list(i: &str) -> IResult<&str, Vec<PathBuf>> {
    separated_list0(tag(":"), parse_path)(i)
}

fn parse_env_var_name(i: &str) -> IResult<&str, EnvVar> {
    let ro = value(EnvVar::RO, tag(ENV_FS_RO_NAME));
    let rw = value(EnvVar::RW, tag(ENV_FS_RW_NAME));
    alt((ro, rw))(i)
}

fn parse_env(input: &str) -> IResult<&str, (EnvVar, Vec<PathBuf>)> {
    tuple((
        parse_env_var_name,
        tag("="),
        delimited(tag("\""), parse_path_list, tag("\"")),
        opt(tag("\n")),
    ))(input)
    .map(|(i, (e, b, p, a))| (i, (e, p)))
}

fn parse_profile(input: &str) -> IResult<&str, Vec<(EnvVar, Vec<PathBuf>)>> {
    all_consuming(many0(parse_env))(input) // , peol_comment))))(input)
}

fn main() -> Result<()> {
    let mut config_base = PathBuf::from(std::env::var("HOME")?);
    config_base.push(".config/landlock");

    let matches = app_from_crate!()
        .setting(AppSettings::TrailingVarArg)
        .arg(Arg::with_name("cmd").required(true))
        .arg(Arg::with_name("args").multiple(true))
        .get_matches();

    let cmd = matches.value_of("cmd").unwrap();
    config_base.push(format!("file_{}.ll", cmd));

    let profile = std::fs::read_to_string(config_base)?;

    dbg!(parse_profile(&profile));

    Ok(())
}
