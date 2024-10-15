use core::panic;
use std::{collections::{HashMap, VecDeque}, io::{self, ErrorKind, Read, Write}, usize};

use crate::common::*;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Cell {
    Unknown,
    Val(u8)
}

struct LoopEnterState {
    tape: VecDeque<Cell>,
    head_pos: usize,
    outputted_head_pos: usize,
    tape_offset: isize, 
    program_counter: usize,
    emitted_insts: Vec<Instruction>,
}

pub struct State {
    tape: VecDeque<Cell>,
    head_pos: usize,
    outputted_head_pos: usize,

    // Because the interpreter can shift the tape when the head goes negative, we need to keep
    // track of how much it's been shifted and account for that when we emit instructions referring
    // to the head's position
    tape_offset: isize, 

    program_counter: usize,
    program: Vec<Instruction>,
    execution_counter: Vec<usize>,

    // If the PC becomes unknown inside of a loop, we need to reset the execution's state to the
    // beginning of the last outermost loop, then begin execution from there.
    loop_enter_state : Option<LoopEnterState>,

    // Also track loop level, so we can clear the loop state when we exit the outermost loop.
    loop_level : i32,

    jump_dests: HashMap<usize, usize>,
}

impl State {
    pub fn new(program: Vec<Instruction>) -> Self {
        let mut t = VecDeque::new();
        t.push_back(Cell::Val(0));

        let execution_counter = vec![0; program.len()];
        let jump_dests = compute_jump_dests(&program);

        State {
            tape: t,
            head_pos: 0,
            outputted_head_pos: 0,
            tape_offset: 0,
            program_counter: 0,
            program,
            execution_counter,
            loop_enter_state: None,
            loop_level: 0,
            jump_dests,
        }
    }

    fn move_right(&mut self) {
        self.head_pos += 1;

        if self.head_pos >= self.tape.len() {
            self.tape.push_back(Cell::Val(0));
        }

        self.program_counter += 1;
    }

    fn move_left(&mut self) {
        if self.head_pos == 0 {
            self.tape.push_front(Cell::Val(0));
            self.tape_offset += 1;
        } else {
            self.head_pos -= 1;
        }

        self.program_counter += 1;
    }

    fn increment(&mut self) {
        match self.tape[self.head_pos] {
            Cell::Unknown => panic!("incremented unknown cell"),
            Cell::Val(x) => self.tape[self.head_pos] = Cell::Val(u8::wrapping_add(x, 1u8))
        }

        self.program_counter += 1;
    }

    fn decrement(&mut self) {
        match self.tape[self.head_pos] {
            Cell::Unknown => panic!("decremented unknown cell"),
            Cell::Val(x) => self.tape[self.head_pos] = Cell::Val(u8::wrapping_sub(x, 1u8))
        }

        self.program_counter += 1;
    }

    fn write(&mut self, mut writer : impl Write) {
        match self.tape[self.head_pos] {
            Cell::Unknown => panic!("wrote unknown cell"),
            Cell::Val(x) => {
                let buf = [x;1];
                writer.write_all(&buf).expect("unable to write buf");
            }
        }

        self.program_counter += 1;
    }

    fn read(&mut self, mut reader : impl Read) {
        // Read a character from stdin
        let mut buf = [0u8; 1];
        let read_res = reader.read_exact(&mut buf);
        match read_res {
            Ok(_) => (),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => buf[0] = 255,
            Err(_) => panic!("Error while reading from stdin!")
        }

        self.tape[self.head_pos] = Cell::Val(buf[0]);

        self.program_counter += 1;
    }

    fn jump_if_zero(&mut self) {
        let curr_value = match self.tape[self.head_pos] {
            Cell::Unknown => panic!("jump if 0 with unknown cell"),
            Cell::Val(x) => x
        };
        
        if curr_value == 0 {
            self.program_counter = self.jump_dests[&self.program_counter];
        } else {
            self.program_counter += 1;
        }
    }

    fn jump_unless_zero(&mut self) {
        let curr_value = match self.tape[self.head_pos] {
            Cell::Unknown => panic!("jump unless 0 with unknown cell"),
            Cell::Val(x) => x
        };
 
        if curr_value != 0 {
            self.program_counter = self.jump_dests[&self.program_counter];
        } else {
            self.program_counter += 1;
        }
    }

    pub fn interp(&mut self, mut reader : impl Read, mut writer : impl Write)
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
                Instruction::Write => self.write(&mut writer),
                Instruction::Read => self.read(&mut reader),
                Instruction::JumpIfZero => self.jump_if_zero(),
                Instruction::JumpUnlessZero => self.jump_unless_zero(),
                _ => panic!("unhandled instruction: {}", self.program[self.program_counter])
            }
        }
    }

    fn sync_compiled_head_pos(&mut self, insts: &mut Vec<Instruction>) {
        if self.head_pos != self.outputted_head_pos {
            let head_pos : i32 = self.head_pos.try_into().unwrap();
            let offset : i32 = self.tape_offset.try_into().unwrap();
            insts.push(Instruction::SetHeadPos(head_pos - offset));
            self.outputted_head_pos = self.head_pos;
        }
    }

    // Evaluate all instructions not tainted by input. After all instructions are evaluated, emit
    // instructions to setup the head and tape state when evaluation has finished.
    pub fn partial_eval(&mut self) -> Vec<Instruction> {
        let mut insts = Vec::new();

        loop {
            if self.program_counter >= self.program.len() {
                break;
            }

            match self.program[self.program_counter] {
                Instruction::MoveRight => self.move_right(),
                Instruction::MoveLeft => self.move_left(),

                Instruction::Increment => {
                    match self.tape[self.head_pos] {
                        Cell::Unknown => {
                            self.sync_compiled_head_pos(&mut insts);
                            insts.push(Instruction::Increment);
                            self.program_counter += 1;
                        }
                        Cell::Val(_) => self.increment(),
                    }
                }

                Instruction::Decrement => {
                    match self.tape[self.head_pos] {
                        Cell::Unknown => {
                            self.sync_compiled_head_pos(&mut insts);
                            insts.push(Instruction::Decrement);
                            self.program_counter += 1;
                        }
                        Cell::Val(_) => self.decrement(),
                    }
                }

                Instruction::Write => {
                    match self.tape[self.head_pos] {
                        Cell::Unknown => {
                            self.sync_compiled_head_pos(&mut insts);
                            insts.push(Instruction::Write);
                        }
                        Cell::Val(x) => insts.push(Instruction::Output(x))
                    };
                    self.program_counter += 1;
                },

                Instruction::Read => {
                    self.sync_compiled_head_pos(&mut insts);

                    self.tape[self.head_pos] = Cell::Unknown;
                    insts.push(Instruction::Read);
                    self.program_counter += 1;
                },

                Instruction::JumpIfZero => {
                    match self.tape[self.head_pos] {
                        // We no longer know the PC. Bail out and compile the rest of the
                        // instructions.
                        Cell::Unknown => break,
                        Cell::Val(_) => {
                            match self.loop_enter_state {
                                None => {
                                    self.loop_enter_state = Some(LoopEnterState{
                                        tape: self.tape.clone(),
                                        head_pos: self.head_pos,
                                        outputted_head_pos: self.outputted_head_pos,
                                        tape_offset: self.tape_offset,
                                        program_counter: self.program_counter,
                                        emitted_insts: insts.clone()
                                    })
                                }
                                Some(_) => (),
                            }
                            self.loop_level += 1;
                            self.jump_if_zero();
                        }
                    }
                }
                Instruction::JumpUnlessZero => {
                    match self.tape[self.head_pos] {
                        Cell::Val(_) => {
                            self.jump_unless_zero();

                            self.loop_level -= 1;
                            if self.loop_level == 0 {
                                self.loop_enter_state = None;
                            }
                        }
                        Cell::Unknown => {
                            break;
                        }
                    }
                }
                _ => panic!("unhandled instruction: {}", self.program[self.program_counter])
            }
        }

        // If we bailed out while inside of a loop, restore the execution state to the point where
        // we entered the outermost loop.
        match &self.loop_enter_state {
            None => (),
            Some(s) => {
                self.tape = s.tape.clone();
                self.head_pos = s.head_pos;
                self.outputted_head_pos = s.outputted_head_pos;
                self.tape_offset = s.tape_offset;
                self.program_counter = s.program_counter;
                insts = s.emitted_insts.clone();
            }
        }

        // We'll be emitting runtime instructions. Write out head and tape state.
        if self.program_counter < self.program.len() {
            self.sync_compiled_head_pos(&mut insts);

            for idx in 0..self.tape.len() {
                match self.tape[idx] {
                    Cell::Unknown => (),
                    Cell::Val(x) => {
                        let idx : i32 = idx.try_into().unwrap();
                        let offset : i32 = self.tape_offset.try_into().unwrap();
                        let offset_idx : i32 = idx - offset;

                        insts.push(Instruction::SetCell(offset_idx, x));
                    }
                }
            }
        }

        // If there are any instructions after this point, simply output them and let the compiler
        // handle them.
        for pc in self.program_counter..self.program.len() {
            insts.push(self.program[pc]);
        }

        return insts;
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
    use std::fs::*;

    #[test]
    fn test_move_right() {
        let program = lex(">");
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.head_pos, 1);
        assert_eq!(state.tape.len(), 2);
    }

    #[test]
    fn test_move_right_resize() {
        let move_amt = 16;
        let program = lex(&(0..move_amt).map(|_| ">").collect::<String>());
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.head_pos, move_amt);
        assert_eq!(state.tape.len(), (move_amt + 1).try_into().unwrap());
    }

    #[test]
    fn test_move_left() {
        let program = lex("><");
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.head_pos, 0);
    }

    #[test]
    fn test_move_left_negative() {
        let program = lex("<+");
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.head_pos, 0);
        assert_eq!(state.tape.len(), 2);
        assert_eq!(state.tape[0], Cell::Val(1));
        assert_eq!(state.tape[1], Cell::Val(0));
    }

    #[test]
    fn test_increment() {
        let program = lex("+");
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(1));
    }

    #[test]
    fn test_decrement() {
        let program = lex("-");
        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(u8::MAX));
    }

    #[test]
    fn test_jump_if_zero1() {
        // Skip increment
        let program = lex("[+]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(0));
    }

    #[test]
    fn test_jump_if_zero2() {
        // Don't skip outer brace, but skipper inner one.
        let program = lex("+[>[>+]>>>]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(1));
        assert_eq!(state.tape[1], Cell::Val(0));
    }

    #[test]
    fn test_jump_if_zero3() {
        let program = lex("+[>++>]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(1));
        assert_eq!(state.tape[1], Cell::Val(2));
    }

    #[test]
    fn test_jump_unless_zero1() {
        // Set loop idx to 5, then increment cell 1 5 times
        let program = lex("+++++[>+<-]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[0], Cell::Val(0));
        assert_eq!(state.tape[1], Cell::Val(5));
    }

    #[test]
    fn test_ctrl_flow1() {
        let program = lex("+++++[>++++++++++[>+<-]<-]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        assert_eq!(state.tape[2], Cell::Val(50));
    }

    #[test]
    fn test_execution_counter() {
        let program = lex("+++++[>+<-]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

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
        state.interp(std::io::stdin(), std::io::stdout());
        
        let (simple_loops, complex_loops) = state.get_loop_executions();
        assert_eq!(simple_loops.len(), 0);
        assert_eq!(complex_loops.len(), 0);
    }

    #[test]
    fn test_get_loop_profile_one_simple() {
        let program = lex(">+++[>+++<-]");

        let mut state = State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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
        state.interp(std::io::stdin(), std::io::stdout());
        
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

    // Tests to add:
    // - all insts in loop are emitted if pc becomes dirty at end of loop
    // - tape state on loop enter is emitted if pc becomes dirty at end of loop
    // - Nested loop becomes dirty on loop enter (outer loop insts + state are emitted)
    // - Nested loop becomes dirty on loop exit (outer loop insts + state are emitted)

    #[test]
    fn test_partial_eval_simple() {
        let program = lex("+.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [Instruction::Output(1)]);
    }

    #[test]
    fn test_partial_eval_read_becomes_unknown() {
        let program = lex(",");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [Instruction::Read]);
        assert_eq!(state.tape[0], Cell::Unknown);
    }

    #[test]
    fn test_partial_eval_known_and_unknown_cells() {
        let program = lex(",>+++.<.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [Instruction::Read, Instruction::Output(3), Instruction::Write]);
        assert_eq!(state.tape, [Cell::Unknown, Cell::Val(3)]);
    }

    #[test]
    fn test_partial_eval_read_inc_write() {
        let program = lex(",+++.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::Read,
            Instruction::Increment,
            Instruction::Increment,
            Instruction::Increment,
            Instruction::Write,
        ]);
        assert_eq!(state.tape, [Cell::Unknown]);
    }

    #[test]
    fn test_partial_eval_negative_head_pos() {
        let program = lex(">>,<<<,.>>>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::SetHeadPos(2),
            Instruction::Read,
            Instruction::SetHeadPos(-1),
            Instruction::Read,
            Instruction::Write,
            Instruction::SetHeadPos(2),
            Instruction::Write,
        ]);
        assert_eq!(state.tape, [Cell::Unknown, Cell::Val(0), Cell::Val(0), Cell::Unknown]);
    }

    #[test]
    fn test_partial_eval_loop() {
        let program = lex("+++[->++<]>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::Output(6),
        ]);
        assert_eq!(state.tape, [Cell::Val(0), Cell::Val(6)]);
    }

    #[test]
    fn test_partial_eval_read_in_loop() {
        let program = lex("+++[->++>,.<<]>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::SetHeadPos(2),
            Instruction::Read,
            Instruction::Write,
            Instruction::Read,
            Instruction::Write,
            Instruction::Read,
            Instruction::Write,
            Instruction::Output(6),
        ]);
        assert_eq!(state.tape, [Cell::Val(0), Cell::Val(6), Cell::Unknown]);
    }

    // TODO: We can recover cell state for a loop index!
    //
    // Example:
    //   ,[->+<]>. 
    //          ^ We know that the index cell will always be zero at this point.
    //
    // BUT: Does it matter? Would we be doing something our loop simplifier already handles?

    #[test]
    fn test_partial_eval_unknown_pc_loop_enter() {
        let program = lex(",[->+<]>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::Read,
            Instruction::JumpIfZero,
            Instruction::Decrement,
            Instruction::MoveRight,
            Instruction::Increment,
            Instruction::MoveLeft,
            Instruction::JumpUnlessZero,
            Instruction::MoveRight,
            Instruction::Write,
        ]);
        assert_eq!(state.tape, [Cell::Unknown]);
    }

    #[test]
    fn test_partial_eval_unknown_pc_loop_enter_nested() {
        let program = lex(">+++[->,[->+<]]>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::SetHeadPos(1),
            Instruction::SetCell(0,0),
            Instruction::SetCell(1,3),
            Instruction::JumpIfZero,
            Instruction::Decrement,
            Instruction::MoveRight,
            Instruction::Read,
            Instruction::JumpIfZero,
            Instruction::Decrement,
            Instruction::MoveRight,
            Instruction::Increment,
            Instruction::MoveLeft,
            Instruction::JumpUnlessZero,
            Instruction::JumpUnlessZero,
            Instruction::MoveRight,
            Instruction::Write
        ]);
        assert_eq!(state.tape, [Cell::Val(0), Cell::Val(3)]);
    }


    #[test]
    fn test_partial_eval_unknown_pc_loop_exit() {
        let program = lex("+>+++[,]<.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::SetHeadPos(1),
            Instruction::SetCell(0, 1),
            Instruction::SetCell(1, 3),
            Instruction::JumpIfZero,
            Instruction::Read,
            Instruction::JumpUnlessZero,
            Instruction::MoveLeft,
            Instruction::Write,

        ]);
        assert_eq!(state.tape, [Cell::Val(1), Cell::Val(3)]);
    }

    // TODO: It would be nice if we only wrote out cell values that are actually used
    #[test]
    fn test_partial_eval_unknown_pc_head_and_tape_state_written() {
        let program = lex("+>++<<+++>>>,[->+<]>.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::SetHeadPos(2),
            Instruction::Read,
            Instruction::SetCell(-1, 3),
            Instruction::SetCell(0, 1),
            Instruction::SetCell(1, 2),
            Instruction::JumpIfZero,
            Instruction::Decrement,
            Instruction::MoveRight,
            Instruction::Increment,
            Instruction::MoveLeft,
            Instruction::JumpUnlessZero,
            Instruction::MoveRight,
            Instruction::Write,
        ]);
        assert_eq!(state.tape, [Cell::Val(3), Cell::Val(1), Cell::Val(2), Cell::Unknown]);
    }

    #[test]
    fn test_partial_eval_read_dec_write() {
        let program = lex(",---.");

        let mut state = State::new(program);
        let insts = state.partial_eval();

        assert_eq!(insts, [
            Instruction::Read,
            Instruction::Decrement,
            Instruction::Decrement,
            Instruction::Decrement,
            Instruction::Write,
        ]);
        assert_eq!(state.tape, [Cell::Unknown]);
    }

    #[test]
    #[ignore]
    fn test_bfcheck() {
        let (progs, outputs, input_path) = get_tests();

        let mut input_file = File::open(input_path).unwrap();
        let mut input = Vec::new();
        input_file.read_to_end(&mut input).unwrap();

        for i in 0..progs.len() {
            let prog_path = progs[i].clone();
            let output_path = outputs[i].clone();

            let input_prog = lex(&std::fs::read_to_string(prog_path.clone()).expect("unable to read file"));
            let mut output = Vec::new();
            let mut input = input.clone();

            let mut state = State::new(input_prog);
            state.interp(&input[..], output.by_ref());

            let mut orig_output = Vec::new();
            let mut output_file = File::open(output_path).unwrap();
            output_file.read_to_end(&mut orig_output).unwrap();

            println!("{}", prog_path.to_str().unwrap());
            assert_eq!(output, orig_output);
        }
    }

}
