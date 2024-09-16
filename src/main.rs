use std::{collections::{HashMap, VecDeque}, env, fs, io::{self, Read}, process::ExitCode, usize};

enum Instruction {
    MoveRight,
    MoveLeft,
    Increment,
    Decrement,
    Write,
    Read,
    JumpIfZero,
    JumpUnlessZero
}

struct State {
    tape: VecDeque<u8>,
    head_pos: usize,

    program_counter: usize,
    program: Vec<Instruction>,
    jump_dests: HashMap<usize, usize>,
}

fn lex(program : &str) -> Vec<Instruction> {
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

fn find_matching_jump_if_zero(insts : &Vec<Instruction>, start_pc : usize) -> usize {
    let mut pc = start_pc + 1;
    let mut brace_count = 1;

    loop {
        match insts[pc] {
            Instruction::JumpIfZero => brace_count += 1,
            Instruction::JumpUnlessZero => brace_count -= 1,
            _ => (),
        }

        if brace_count == 0 {
            return pc;
        }

        pc += 1;
    }
}

fn find_matching_jump_unless_zero(insts : &Vec<Instruction>, start_pc : usize) -> usize {
    let mut pc = start_pc - 1;
    let mut brace_count = 1;

    loop {
        match insts[pc] {
            Instruction::JumpUnlessZero => brace_count += 1,
            Instruction::JumpIfZero => brace_count -= 1,
            _ => (),
        }

        if brace_count == 0 {
            return pc;
        }

        pc -= 1;
    }
}

fn compute_jump_dests(insts : &Vec<Instruction>) -> HashMap<usize, usize> {
    let mut jump_dests = HashMap::new();

    for pc in 0..insts.len() {
        match insts[pc] {
            Instruction::JumpIfZero => {
                jump_dests.insert(pc, find_matching_jump_if_zero(&insts, pc));
                ()
            },
            Instruction::JumpUnlessZero => {
                jump_dests.insert(pc, find_matching_jump_unless_zero(&insts, pc));
                ()
            }
            _ => (),
        }
    }

    return jump_dests;
}

impl State {
    pub fn new(input: &str) -> Self {
        let mut t = VecDeque::new();
        t.push_back(0);

        let program = lex(input);
        let jump_dests = compute_jump_dests(&program);

        State {
            tape: t,
            head_pos: 0,
            program_counter: 0,
            program,
            jump_dests,
        }
    }

    fn move_right(&mut self) {
        self.head_pos += 1;

        if self.head_pos >= self.tape.len() {
            self.tape.push_back(0);
        }

        self.program_counter += 1;
    }

    fn move_left(&mut self) {
        if self.head_pos == 0 {
            self.tape.push_front(0);
        } else {
            self.head_pos -= 1;
        }

        self.program_counter += 1;
    }

    fn increment(&mut self) {
        let curr = self.tape[self.head_pos];
        self.tape[self.head_pos] = u8::wrapping_add(curr, 1u8);

        self.program_counter += 1;
    }

    fn decrement(&mut self) {
        self.tape[self.head_pos] = u8::wrapping_sub(self.tape[self.head_pos], 1u8);

        self.program_counter += 1;
    }

    fn write(&mut self) {
        let msg = char::from_u32(self.tape[self.head_pos].into()).unwrap();
        print!("{}", msg);

        self.program_counter += 1;
    }

    fn read(&mut self) {
        // Read a character from stdin
        let mut buf = [0u8; 1];
        let _ = io::stdin().read_exact(&mut buf);

        self.tape[self.head_pos] = buf[0];

        self.program_counter += 1;
    }

    fn jump_if_zero(&mut self) {
        let curr_value = self.tape[self.head_pos];
        
        if curr_value == 0 {
            self.program_counter = self.jump_dests[&self.program_counter];
        } else {
            self.program_counter += 1;
        }
    }

    fn jump_unless_zero(&mut self) {
        let curr_value = self.tape[self.head_pos];

        if curr_value != 0 {
            self.program_counter = self.jump_dests[&self.program_counter];
        } else {
            self.program_counter += 1;
        }
    }

    fn interp(&mut self)
    {
        loop {
            if self.program_counter >= self.program.len() {
                break;
            }

            match self.program[self.program_counter] {
                Instruction::MoveRight => self.move_right(),
                Instruction::MoveLeft => self.move_left(),
                Instruction::Increment => self.increment(),
                Instruction::Decrement => self.decrement(),
                Instruction::Write => self.write(),
                Instruction::Read => self.read(),
                Instruction::JumpIfZero => self.jump_if_zero(),
                Instruction::JumpUnlessZero => self.jump_unless_zero(),
            }
        }
    }
}

fn print_usage() {
    println!("Usage: brainbug [path to bf file]");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        print_usage();
        return ExitCode::from(1);
    }

    let program = fs::read_to_string(args[1].clone()).expect("unable to read file");
    let mut state = State::new(&program);
    state.interp();

    return ExitCode::from(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_right() {
        let program = ">";
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.head_pos, 1);
        assert_eq!(state.tape.len(), 2);
    }

    #[test]
    fn test_move_right_resize() {
        let move_amt = 16;
        let program = (0..move_amt).map(|_| ">").collect::<String>();
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.head_pos, move_amt);
        assert_eq!(state.tape.len(), (move_amt + 1).try_into().unwrap());
    }

    #[test]
    fn test_move_left() {
        let program = "><";
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.head_pos, 0);
    }

    #[test]
    fn test_move_left_negative() {
        let program = "<+";
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.head_pos, 0);
        assert_eq!(state.tape.len(), 2);
        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 0);
    }

    #[test]
    fn test_increment() {
        let program = "+";
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], 1);
    }

    #[test]
    fn test_decrement() {
        let program = "-";
        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], u8::MAX);
    }

    #[test]
    fn test_jump_if_zero1() {
        // Skip increment
        let program = "[+]";

        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], 0);
    }

    #[test]
    fn test_jump_if_zero2() {
        // Don't skip outer brace, but skipper inner one.
        let program = "+[>[>+]>>>]";

        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 0);
    }

    #[test]
    fn test_jump_if_zero3() {
        let program = "+[>++>]";

        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 2);
    }

    #[test]
    fn test_jump_unless_zero1() {
        // Set loop idx to 5, then increment cell 1 5 times
        let program = "+++++[>+<-]";

        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[0], 0);
        assert_eq!(state.tape[1], 5);
    }

    #[test]
    fn test_ctrl_flow1() {
        let program = "+++++[>++++++++++[>+<-]<-]";

        let mut state = State::new(&program);
        state.interp();

        assert_eq!(state.tape[2], 50);
    }

}
