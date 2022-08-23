// Implementing ToString for Statement enum so that making full latex text easily.

use crate::error;
use crate::parser::ast::*;
use crate::parser::Parser;

pub fn make_latex_format<const IS_TEST: bool>(parser: &mut Parser) -> error::Result<String> {
    let latex = parser.parse_latex()?;
    let mut output = String::new();

    if !IS_TEST {
        output += &format!(
            "%\n%  This file was generated by vesti {}\n%\n",
            env!("CARGO_PKG_VERSION")
        );
    }

    for stmt in latex {
        output += &stmt.to_string();
    }

    Ok(output)
}

impl ToString for Statement {
    fn to_string(&self) -> String {
        match self {
            Statement::DocumentClass { name, options } => docclass_to_string(name, options),
            Statement::Usepackage { name, options } => usepackage_to_string(name, options),
            Statement::MultiUsepackages { pkgs } => multiusepacakge_to_string(pkgs),
            Statement::DocumentStart => String::from("\\begin{document}\n"),
            Statement::DocumentEnd => String::from("\n\\end{document}\n"),
            Statement::MainText(s) => s.clone(),
            Statement::PlainTextInMath(latex) => plaintext_in_math_to_string(latex),
            Statement::Integer(i) => i.to_string(),
            Statement::Float(f) => f.to_string(),
            Statement::RawLatex(s) => s.clone(),
            Statement::MathText { state, text } => math_text_to_string(*state, text),
            Statement::LatexFunction { name, args } => latex_function_to_string(name, args),
            Statement::Environment { name, args, text } => environment_to_string(name, args, text),
            Statement::BeginPhantomEnvironment { name, args } => {
                begin_phantom_environment_to_string(name, args)
            }
            Statement::EndPhantomEnvironment { name } => format!("\\end{{{name}}}"),
            Statement::FunctionDefine {
                style,
                name,
                args,
                trim,
                body,
            } => function_def_to_string(style, name, args, trim, body),
            Statement::EnvironmentDefine {
                is_redefine,
                name,
                args_num,
                optional_arg,
                trim,
                begin_part,
                end_part,
            } => environment_def_to_string(
                *is_redefine,
                name,
                *args_num,
                optional_arg.as_ref(),
                trim,
                begin_part,
                end_part,
            ),
        }
    }
}

fn docclass_to_string(name: &str, options: &Option<Vec<Latex>>) -> String {
    if let Some(opts) = options {
        let mut options_str = String::new();
        for o in opts {
            options_str = options_str + &latex_to_string(o) + ",";
        }
        options_str.pop();

        format!("\\documentclass[{options_str}]{{{name}}}\n")
    } else {
        format!("\\documentclass{{{name}}}\n")
    }
}

fn usepackage_to_string(name: &str, options: &Option<Vec<Latex>>) -> String {
    if let Some(opts) = options {
        let mut options_str = String::new();
        for o in opts {
            options_str = options_str + &latex_to_string(o) + ",";
        }
        options_str.pop();

        format!("\\usepackage[{options_str}]{{{name}}}\n")
    } else {
        format!("\\usepackage{{{name}}}\n")
    }
}

fn multiusepacakge_to_string(pkgs: &[Statement]) -> String {
    let mut output = String::new();
    for pkg in pkgs {
        if let Statement::Usepackage { name, options } = pkg {
            output += &usepackage_to_string(name, options);
        }
    }
    output
}

fn math_text_to_string(state: MathState, text: &[Statement]) -> String {
    let mut output = String::new();
    match state {
        MathState::Text => {
            output += "$";
            for t in text {
                output += &t.to_string();
            }
            output += "$";
        }
        MathState::Inline => {
            output += "\\[";
            for t in text {
                output += &t.to_string();
            }
            output += "\\]";
        }
    }
    output
}

fn plaintext_in_math_to_string(latex: &Latex) -> String {
    let mut output = latex_to_string(latex);
    if output.as_bytes()[output.len() - 1] == b' ' {
        output.pop();
    }

    format!("\\text{{{}}}", output)
}

fn latex_function_to_string(name: &str, args: &Vec<(ArgNeed, Vec<Statement>)>) -> String {
    let mut output = format!("{}", name);
    for arg in args {
        let mut tmp = String::new();
        for t in &arg.1 {
            tmp += &t.to_string();
        }
        match arg.0 {
            ArgNeed::MainArg => output += &format!("{{{tmp}}}"),
            ArgNeed::Optional => output += &format!("[{tmp}]"),
            ArgNeed::StarArg => output.push('*'),
        }
    }
    output
}

fn begin_phantom_environment_to_string(
    name: &str,
    args: &Vec<(ArgNeed, Vec<Statement>)>,
) -> String {
    let mut output = format!("\\begin{{{name}}}");
    for arg in args {
        let mut tmp = String::new();
        for t in &arg.1 {
            tmp += &t.to_string();
        }
        match arg.0 {
            ArgNeed::MainArg => output += &format!("{{{tmp}}}"),
            ArgNeed::Optional => output += &format!("[{tmp}]"),
            ArgNeed::StarArg => output.push('*'),
        }
    }
    output
}

fn environment_to_string(
    name: &str,
    args: &Vec<(ArgNeed, Vec<Statement>)>,
    text: &Latex,
) -> String {
    let mut output = format!("\\begin{{{name}}}");
    for arg in args {
        let mut tmp = String::new();
        for t in &arg.1 {
            tmp += &t.to_string();
        }
        match arg.0 {
            ArgNeed::MainArg => output += &format!("{{{tmp}}}"),
            ArgNeed::Optional => output += &format!("[{tmp}]"),
            ArgNeed::StarArg => output.push('*'),
        }
    }
    for t in text {
        output += &t.to_string();
    }
    output += &format!("\\end{{{name}}}\n");
    output
}

fn latex_to_string(latex: &Latex) -> String {
    let mut output = String::new();
    for l in latex {
        output += &l.to_string();
    }
    output
}

fn function_def_to_string(
    style: &FunctionStyle,
    name: &str,
    args: &str,
    trim: &TrimWhitespace,
    body: &Latex,
) -> String {
    let mut output = match style {
        FunctionStyle::Plain => format!("\\def\\{name}{args}{{"),
        FunctionStyle::LongPlain => format!("\\long\\def\\{name}{args}{{"),
        FunctionStyle::OuterPlain => format!("\\outer\\def\\{name}{args}{{"),
        FunctionStyle::LongOuterPlain => format!("\\long\\outer\\def\\{name}{args}{{"),
        FunctionStyle::Expand => format!("\\edef\\{name}{args}{{"),
        FunctionStyle::LongExpand => format!("\\long\\edef\\{name}{args}{{"),
        FunctionStyle::OuterExpand => format!("\\outer\\edef\\{name}{args}{{"),
        FunctionStyle::LongOuterExpand => format!("\\long\\outer\\edef\\{name}{args}{{"),
        FunctionStyle::Global => format!("\\gdef\\{name}{args}{{"),
        FunctionStyle::LongGlobal => format!("\\long\\gdef\\{name}{args}{{"),
        FunctionStyle::OuterGlobal => format!("\\outer\\gdef\\{name}{args}{{"),
        FunctionStyle::LongOuterGlobal => format!("\\long\\outer\\gdef\\{name}{args}{{"),
        FunctionStyle::ExpandGlobal => format!("\\xdef\\{name}{args}{{"),
        FunctionStyle::LongExpandGlobal => format!("\\long\\xdef\\{name}{args}{{"),
        FunctionStyle::OuterExpandGlobal => format!("\\outer\\xdef\\{name}{args}{{"),
        FunctionStyle::LongOuterExpandGlobal => format!("\\long\\outer\\xdef\\{name}{args}{{"),
    };

    let mut tmp = String::new();
    for b in body {
        tmp += &b.to_string();
    }

    output += match (trim.start, trim.end) {
        (false, false) => tmp.as_str(),
        (true, false) => tmp.trim_start(),
        (false, true) => tmp.trim_end(),
        (true, true) => tmp.trim(),
    };
    output.push_str("}\n");

    output
}

fn environment_def_to_string(
    is_redefine: bool,
    name: &str,
    args_num: u8,
    optional_arg: Option<&Latex>,
    trim: &TrimWhitespace,
    begin_part: &Latex,
    end_part: &Latex,
) -> String {
    let mut output = if is_redefine {
        format!("\\renewenvironment{{{name}}}")
    } else {
        format!("\\newenvironment{{{name}}}")
    };

    if args_num > 0 {
        output += &format!("[{args_num}]");
        if let Some(inner) = optional_arg {
            output.push('[');
            for stmt in inner {
                output += &stmt.to_string();
            }
            output.push_str("]{");
        } else {
            output.push('{');
        }
    } else {
        output.push('{');
    }

    let mut tmp = String::new();
    for b in begin_part {
        tmp += &b.to_string();
    }
    output += match (trim.start, trim.mid) {
        (false, Some(false)) => tmp.as_str(),
        (true, Some(false)) => tmp.trim_start(),
        (false, Some(true)) => tmp.trim_end(),
        (true, Some(true)) => tmp.trim(),
        _ => unreachable!("VESTI BUG!!!!: codegen::environment_def_to_string"),
    };
    output.push_str("}{");

    tmp.clear();
    for b in end_part {
        tmp += &b.to_string();
    }
    output += match (trim.mid, trim.end) {
        (Some(false), false) => tmp.as_str(),
        (Some(true), false) => tmp.trim_start(),
        (Some(false), true) => tmp.trim_end(),
        (Some(true), true) => tmp.trim(),
        _ => unreachable!("VESTI BUG!!!!: codegen::environment_def_to_string"),
    };
    output.push_str("}\n");

    output
}
