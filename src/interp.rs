use std::{collections::{HashMap, VecDeque}, io::{self, Read}, usize};

use crate::common::*;

pub struct State {
    tape: VecDeque<u8>,
    head_pos: usize,

    program_counter: usize,
    program: Vec<Instruction>,
    execution_counter: Vec<usize>,

    jump_dests: HashMap<usize, usize>,
}

impl State {
    pub fn new(program: Vec<Instruction>) -> Self {
        let mut t = VecDeque::new();
        t.push_back(0);

        let execution_counter = vec![0; program.len()];
        let jump_dests = compute_jump_dests(&program);

        State {
            tape: t,
            head_pos: 0,
            program_counter: 0,
            program,
            execution_counter,
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

    pub fn interp(&mut self)
    {
        loop {
            if self.program_counter >= self.program.len() {
                break;
            }

            self.execution_counter[self.program_counter] += 1;

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

    fn get_loop_executions(&self) -> (Vec<LoopExecution>, Vec<LoopExecution>) {
        let mut simple_loops : Vec<LoopExecution> = Vec::new();
        let mut complex_loops : Vec<LoopExecution> = Vec::new();

        let mut curr_loop : Option<LoopExecution> = Option::None;
        let mut has_io = false;
        let mut pointer_offset : i32 = 0;
        let mut pointer_value : i32 = 0;

        for pc in 0..self.program.len() {
            match curr_loop {
                Option::None => (),
                Option::Some(ref mut l) => l.insts.push(self.program[pc].clone()),
            }

            match self.program[pc] {
                Instruction::MoveRight => pointer_offset += 1,
                Instruction::MoveLeft => pointer_offset -= 1,
                Instruction::Write | Instruction::Read => has_io = true,

                Instruction::Increment => {
                    if pointer_offset == 0 {
                        pointer_value += 1;
                    }
                },
                Instruction::Decrement => {
                    if pointer_offset == 0 {
                        pointer_value -= 1;
                    }
                },

                _ => (),
            }

            match self.program[pc] {
                Instruction::JumpIfZero => {
                    curr_loop = Some(LoopExecution{pc, num_times_executed: 0, insts: vec![Instruction::JumpIfZero;1]});
                    has_io = false;
                    pointer_offset = 0;
                    pointer_value = 0;
                },

                Instruction::JumpUnlessZero => {
                    let curr = curr_loop;
                    curr_loop = Option::None;

                    let index_changed_by_1 = pointer_value.abs() == 1;

                    match curr {
                        Option::None => (),
                        Option::Some(l) => {
                            if !has_io && pointer_offset == 0 && index_changed_by_1 {
                                simple_loops.push(l);
                            } else {
                                complex_loops.push(l);
                            }
                        }
                    }
                },

                _ => {
                    match curr_loop {
                        Option::None => (),
                        Option::Some(ref mut l) => {
                            if l.num_times_executed == 0 {
                                l.num_times_executed = self.execution_counter[pc];
                            }
                        },
                    }
                },
            }
        }

        simple_loops.sort();
        complex_loops.sort();

        return (simple_loops, complex_loops);
    }

    pub fn print_profile_info(&mut self)
    {
        println!("PC\tOP\t# EXECUTED");
        for pc in 0..self.program.len() {
            println!("{}\t{}\t{}", pc, self.program[pc], self.execution_counter[pc]);
        }

        let (simple_loops, complex_loops) = self.get_loop_executions();

        println!("\nSIMPLE LOOPS");
        println!("PC\t# EXECUTED\tINSTS");
        for l in simple_loops {
            print!("{}\t{}\t", l.pc, l.num_times_executed);
            for i in l.insts {
                print!("{}", i);
            }
            print!("\n");
        }

        println!("\nCOMPLEX LOOPS");
        println!("PC\t# EXECUTED\tINSTS");
        for l in complex_loops {
            print!("{}\t{}\t", l.pc, l.num_times_executed);
            for i in l.insts {
                print!("{}", i);
            }
            print!("\n");
        }
    }
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

#[derive(Eq)]
struct LoopExecution {
    pc : usize,
    num_times_executed : usize,
    insts : Vec<Instruction>,
}

impl Ord for LoopExecution {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.num_times_executed.cmp(&self.num_times_executed)
    }
}

impl PartialOrd for LoopExecution {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for LoopExecution {
    fn eq(&self, other: &Self) -> bool {
        self.num_times_executed == other.num_times_executed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_right() {
        let program = lex(">");
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.head_pos, 1);
        assert_eq!(state.tape.len(), 2);
    }

    #[test]
    fn test_move_right_resize() {
        let move_amt = 16;
        let program = lex(&(0..move_amt).map(|_| ">").collect::<String>());
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.head_pos, move_amt);
        assert_eq!(state.tape.len(), (move_amt + 1).try_into().unwrap());
    }

    #[test]
    fn test_move_left() {
        let program = lex("><");
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.head_pos, 0);
    }

    #[test]
    fn test_move_left_negative() {
        let program = lex("<+");
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.head_pos, 0);
        assert_eq!(state.tape.len(), 2);
        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 0);
    }

    #[test]
    fn test_increment() {
        let program = lex("+");
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], 1);
    }

    #[test]
    fn test_decrement() {
        let program = lex("-");
        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], u8::MAX);
    }

    #[test]
    fn test_jump_if_zero1() {
        // Skip increment
        let program = lex("[+]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], 0);
    }

    #[test]
    fn test_jump_if_zero2() {
        // Don't skip outer brace, but skipper inner one.
        let program = lex("+[>[>+]>>>]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 0);
    }

    #[test]
    fn test_jump_if_zero3() {
        let program = lex("+[>++>]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], 1);
        assert_eq!(state.tape[1], 2);
    }

    #[test]
    fn test_jump_unless_zero1() {
        // Set loop idx to 5, then increment cell 1 5 times
        let program = lex("+++++[>+<-]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[0], 0);
        assert_eq!(state.tape[1], 5);
    }

    #[test]
    fn test_ctrl_flow1() {
        let program = lex("+++++[>++++++++++[>+<-]<-]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.tape[2], 50);
    }

    #[test]
    fn test_execution_counter() {
        let program = lex("+++++[>+<-]");

        let mut state = State::new(program);
        state.interp();

        assert_eq!(state.execution_counter[0], 1);
        assert_eq!(state.execution_counter[1], 1);
        assert_eq!(state.execution_counter[2], 1);
        assert_eq!(state.execution_counter[3], 1);
        assert_eq!(state.execution_counter[4], 1);

        assert_eq!(state.execution_counter[5], 5);
        assert_eq!(state.execution_counter[6], 5);
        assert_eq!(state.execution_counter[7], 5);
        assert_eq!(state.execution_counter[8], 5);
        assert_eq!(state.execution_counter[9], 5);
    }

    #[test]
    fn test_get_loop_profile_no_loops() {
        let program = lex("+++++");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);
        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_one_simple() {
        let program = lex(">+++[>+++<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 1);
        assert_eq!(simple_loops[0].pc, 4);
        assert_eq!(simple_loops[0].num_times_executed, 3);

        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_one_simple_no_exe() {
        let program = lex("+++>[>+++<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 1);
        assert_eq!(simple_loops[0].pc, 4);
        assert_eq!(simple_loops[0].num_times_executed, 0);

        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_one_complex_io() {
        let program = lex(">+++[>.+++<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);

        assert_eq!(complex_loops.len(), 1);
        assert_eq!(complex_loops[0].pc, 4);
        assert_eq!(complex_loops[0].num_times_executed, 3);
    }

    #[test]
    fn test_get_loop_profile_one_complex_pointer_offset_change() {
        let program = lex(">+++[>]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);

        assert_eq!(complex_loops.len(), 1);
        assert_eq!(complex_loops[0].pc, 4);
        assert_eq!(complex_loops[0].num_times_executed, 1);
    }

    #[test]
    fn test_get_loop_profile_one_complex_index() {
        let program = lex(">++++[>+<--]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);

        assert_eq!(complex_loops.len(), 1);
        assert_eq!(complex_loops[0].pc, 5);
        assert_eq!(complex_loops[0].num_times_executed, 2);
    }

    #[test]
    fn test_get_loop_profile_simple_nested() {
        let program = lex(">+++[>+++++[>++<-]<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 1);
        assert_eq!(simple_loops[0].pc, 11);
        assert_eq!(simple_loops[0].num_times_executed, 15);

        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_complex_nested() {
        let program = lex(">+++[>++++++[>++<--]<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);

        assert_eq!(complex_loops.len(), 1);
        assert_eq!(complex_loops[0].pc, 12);
        assert_eq!(complex_loops[0].num_times_executed, 9);
    }

    #[test]
    fn test_get_loop_profile_simple_sorted() {
        let program = lex("+++[>--<-]++[>--<-]++++[>--<-]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 3);
        assert_eq!(simple_loops[0].pc, 23);
        assert_eq!(simple_loops[0].num_times_executed, 4);
        assert_eq!(simple_loops[1].pc, 3);
        assert_eq!(simple_loops[1].num_times_executed, 3);
        assert_eq!(simple_loops[2].pc, 12);
        assert_eq!(simple_loops[2].num_times_executed, 2);

        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_complex_sorted() {
        let program = lex("++++[>--<--]++[>--<--]++++++[>--<--]");

        let mut state = State::new(program);
        state.interp();
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);

        assert_eq!(complex_loops.len(), 3);
        assert_eq!(complex_loops[0].pc, 28);
        assert_eq!(complex_loops[0].num_times_executed, 3);
        assert_eq!(complex_loops[1].pc, 4);
        assert_eq!(complex_loops[1].num_times_executed, 2);
        assert_eq!(complex_loops[2].pc, 14);
        assert_eq!(complex_loops[2].num_times_executed, 1);
    }
}
