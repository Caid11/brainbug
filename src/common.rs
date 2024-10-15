use std::fmt;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::fs;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Instruction {
    MoveRight,
    MoveLeft,
    Increment,
    Decrement,
    Write,
    Read,
    JumpIfZero,
    JumpUnlessZero,
    Zero,

    // Add or subtract the contents at the current cell to the cell at the given offset.
    Add(i32), 
    Sub(i32), 

    // Scan until the head reaches a cell containing 0, moving the head by the specified number of cells each iteration
    Scan(i32),

    // Set the head to the given position
    SetHeadPos(i32),

    // Set the cell to the given value
    SetCell(i32, u8),

    // Output the given literal value
    Output(u8),

    Nop
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
            Instruction::Add(offset) => write!(f, "ADD({offset})"),
            Instruction::Sub(offset) => write!(f, "SUB({offset})"),
            Instruction::Scan(x) => write!(f, "SCAN({x})"),
            Instruction::Nop => write!(f, "NOP"),
            Instruction::Zero => write!(f, "ZERO"),
            Instruction::SetHeadPos(x) => write!(f, "SETHEADPOS({x})"),
            Instruction::SetCell(pos, val) => write!(f, "SETCELL({pos}, {val})"),
            Instruction::Output(val) => write!(f, "OUTPUT({val})")
        }
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
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


