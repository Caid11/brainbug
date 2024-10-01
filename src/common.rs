use std::fmt;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::fs;

#[derive(PartialEq, Eq, Clone)]
pub enum Instruction {
    MoveRight,
    MoveLeft,
    Increment,
    Decrement,
    Write,
    Read,
    JumpIfZero,
    JumpUnlessZero
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::MoveRight => write!(f, ">"),
            Instruction::MoveLeft => write!(f, "<"),
            Instruction::Increment => write!(f, "+"),
            Instruction::Decrement => write!(f, "-"),
            Instruction::Write => write!(f, "."),
            Instruction::Read => write!(f, ","),
            Instruction::JumpIfZero => write!(f, "["),
            Instruction::JumpUnlessZero => write!(f, "]"),
        }
    }
}

pub fn lex(program : &str) -> Vec<Instruction> {
    let mut insts = Vec::new();

    for c in program.chars() {
        match c {
            '>' => insts.push(Instruction::MoveRight),
            '<' => insts.push(Instruction::MoveLeft),
            '+' => insts.push(Instruction::Increment),
            '-' => insts.push(Instruction::Decrement),
            '.' => insts.push(Instruction::Write),
            ',' => insts.push(Instruction::Read),
            '[' => insts.push(Instruction::JumpIfZero),
            ']' => insts.push(Instruction::JumpUnlessZero),
            _ => ()
        }
    }

    return insts;
}

pub fn get_tests() -> (Vec<PathBuf>, Vec<PathBuf>, PathBuf) {
        let bfcheck_path_str = std::env::var("BFCHECK_PATH").expect("must set BFCHECK_PATH");
        let bfcheck_path = Path::new(&bfcheck_path_str);

        let mut progs = Vec::new();
        let mut outputs = Vec::new();

        let prog_re = Regex::new("prog-[0-9]+\\.b").unwrap();
        let output_re = Regex::new("output-[0-9]+\\.dat").unwrap();

        for entry in fs::read_dir(bfcheck_path).unwrap() {
            let entry = entry.unwrap();

            if prog_re.is_match(entry.path().to_str().unwrap()) {
                progs.push(entry.path());
            } else if output_re.is_match(entry.path().to_str().unwrap()) {
                outputs.push(entry.path());
            }
        }

        assert_eq!(progs.len(), outputs.len());

        progs.sort();
        outputs.sort();

        return (progs, outputs, bfcheck_path.join("input.dat"))
    }


