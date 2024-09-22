use std::fmt;

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
