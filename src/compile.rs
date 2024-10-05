use tempfile::{tempfile, NamedTempFile};
use core::panic;
use std::error;
use std::io::{Write};
use std::fs::{File};
use std::process::{Command, Stdio, Output};
use std::fmt;
use std::collections::{HashMap, HashSet};

use crate::common::*;

const TEST_RUNNER : &str = "
#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <io.h>

extern void bf_main( unsigned char* tape );

int main(int argc, char** argv)
{
    // Don't interpret ctrl z as EOF.
    _setmode(0,_O_BINARY);
    _setmode(1,_O_BINARY);

    unsigned char* tape = calloc(4000000, sizeof(char));
    bf_main( tape + 2000000 );
    free(tape);
    fprintf(stderr, \"Exited successfully\\n\");
}
";

const FUNC_BEGIN : &str = "
	.text
	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
.set @feat.00, 0
	.def	bf_main;
	.scl	2;
	.type	32;
	.endef

	.globl	__ymm@ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00
	.section	.rdata,\"dr\",discard,__ymm@ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00
	.p2align	5, 0x0
__ymm@ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00ffffff00:
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff

	.globl	__real@ffffff00
	.section	.rdata,\"dr\",discard,__real@ffffff00
	.p2align	2, 0x0
__real@ffffff00:
	.byte	0                               # 0x0
	.byte	255                             # 0xff
	.byte	255                             # 0xff
	.byte	255                             # 0xff
";

const FUNC_PROLOGUE : &str = "
	.text
	.globl	bf_main
	.p2align	4, 0x90
bf_main:
.seh_proc bf_main
	pushq	%r12
	.seh_pushreg %r12
	subq	$32, %rsp
	.seh_stackalloc 32
	.seh_endprologue

	movq %rcx, %r12

";

const FUNC_END : &str = "
	addq	$32, %rsp
	popq	%r12
	retq
	.seh_endproc

	.addrsig
";

const READ_CHAR : &str = "
	callq getchar
    movb %al, (%r12)
";

const WRITE_CHAR : &str = "
    movzbl (%r12), %ecx
	callq putchar
";

const INCREMENT : &str = "
    incb (%r12)
";

const DECREMENT : &str = "
    decb (%r12)
";

const MOVE_RIGHT : &str = "
    incq %r12
";

const MOVE_LEFT : &str = "
    decq %r12
";

const ZERO : &str = "
    movb $0, (%r12)
";

struct LoopState {
    start_pc : usize,
    
    head_delta : i32,
    ptr_changes : HashMap<i32, i32>
}

fn simplify_loops( program : &mut Vec<Instruction>) {
    let mut in_loop = false;
    let mut curr_loop = LoopState {
        start_pc: 0,
        head_delta: 0,
        ptr_changes: HashMap::new()
    };

    for pc in 0..program.len() {
        let inst = program[pc];

        match inst {
            Instruction::JumpIfZero => {
                curr_loop = LoopState {
                    start_pc: pc,
                    head_delta: 0,
                    ptr_changes: HashMap::new()
                };

                in_loop = true;
            },

            Instruction::JumpUnlessZero => {
                if in_loop {
                    in_loop = false;

                    if curr_loop.head_delta != 0 {
                        continue;
                    }

                    if !curr_loop.ptr_changes.contains_key(&0) 
                        || (curr_loop.ptr_changes[&0] != 1  && curr_loop.ptr_changes[&0] != -1) {
                        continue;
                    }

                    let decrement_loop = curr_loop.ptr_changes[&0] == -1;

                    for i in curr_loop.start_pc..(pc + 1) {
                        program[i] = Instruction::Nop;
                    }

                    let mut write_pc = curr_loop.start_pc;

                    let mut head_deltas : Vec<&i32> = curr_loop.ptr_changes.keys().collect();
                    head_deltas.sort();

                    for head_delta in head_deltas {
                        if *head_delta == 0 {
                            continue;
                        }

                        let value_delta = curr_loop.ptr_changes[head_delta];

                        for i in 0..(value_delta).abs() {
                            if decrement_loop {
                                if value_delta > 0 {
                                    program[write_pc] = Instruction::Add(head_delta.clone());
                                }
                                else if value_delta < 0 {
                                    program[write_pc] = Instruction::Sub(head_delta.clone());
                                }
                            } else {
                                if value_delta > 0 {
                                    program[write_pc] = Instruction::Sub(head_delta.clone());
                                } else if value_delta < 0 {
                                    program[write_pc] = Instruction::Add(head_delta.clone());
                                }
                            }
                            write_pc += 1;
                        }
                    }

                    program[write_pc] = Instruction::Zero;
                }
            }

            Instruction::Read | Instruction::Write => in_loop = false,

            Instruction::MoveLeft => {
                if in_loop {
                    curr_loop.head_delta -= 1;
                }
            },

            Instruction::MoveRight => {
                if in_loop {
                    curr_loop.head_delta += 1;
                }
            },

            Instruction::Increment => {
                if in_loop {
                    let curr = curr_loop.ptr_changes.entry(curr_loop.head_delta).or_insert(0);
                    *curr += 1;
                }
            }

            Instruction::Decrement => {
                if in_loop {
                    let curr = curr_loop.ptr_changes.entry(curr_loop.head_delta).or_insert(0);
                    *curr -= 1;
                }
            }

            _ => in_loop = false,
            }
        }
}

fn vectorize_scans( program : &mut Vec<Instruction>) {
    let mut in_loop = false;
    let mut head_delta : i32  = 0;
    let mut start_pc : usize = 0;

    for pc in 0..program.len() {
        let inst = program[pc];

        match inst {
            Instruction::JumpIfZero => {
                in_loop = true;
                head_delta = 0;
                start_pc = pc;
            },

            Instruction::JumpUnlessZero => {
                if in_loop {
                    in_loop = false;

                    // TODO(Kincaid): Eventually handle left scans
                    if head_delta < 1 {
                        continue;
                    }

                    for i in start_pc..(pc + 1) {
                        program[i] = Instruction::Nop;
                    }

                    program[start_pc] = Instruction::Scan(head_delta);
                }
            }

            Instruction::MoveLeft => {
                if in_loop {
                    head_delta -= 1;
                }
            },

            Instruction::MoveRight => {
                if in_loop {
                    head_delta += 1;
                }
            },

            _ => in_loop = false,
        }
    }
}

pub fn compile_to_asm( input : &mut Vec<Instruction>, do_simplify_loops : bool, do_simplify_scans : bool ) -> String {
    if do_simplify_loops {
        simplify_loops(input);
    }

    if do_simplify_scans {
        vectorize_scans(input);
    }

    let mut globals : String = "".to_owned();
    let mut instructions = "".to_owned();

    let mut curr_label_num = 0;
    let mut label_stack = vec![0;0];

    let mut generated_indices : HashSet<i32> = HashSet::new();

    for inst in input {
        match inst {
            Instruction::MoveRight => instructions += MOVE_RIGHT,
            Instruction::MoveLeft => instructions += MOVE_LEFT,
            Instruction::Increment => instructions += INCREMENT,
            Instruction::Decrement => instructions += DECREMENT,
            Instruction::Read => instructions += READ_CHAR,
            Instruction::Write => instructions += WRITE_CHAR,

            Instruction::JumpIfZero => {
                let new_label_num = curr_label_num;
                curr_label_num += 1;
                label_stack.push(new_label_num);

                instructions += "\n";

                // Generate a jump to the end label.
                instructions += "\tcmpb $0, (%r12)\n";
                instructions += &("\tje .UZ".to_owned() + &new_label_num.to_string() + "\n");

                // Generate a label so corresponding jump unless zero can jump back.
                instructions += &(".IZ".to_owned() + &new_label_num.to_string() + ":\n");
            },

            Instruction::JumpUnlessZero => {
                instructions += "\n";

                // Get the current brace from the stack.
                let label_num = label_stack.pop().unwrap();

                // Generate a jump to the start label.
                instructions += "\tcmpb $0, (%r12)\n";
                instructions += &("\tjne .IZ".to_owned() + &label_num.to_string() + "\n");

                // Generate a label so corresponding jump if zero can jump back.
                instructions += &(".UZ".to_owned() + &label_num.to_string() + ":\n");
            },

            Instruction::Zero => instructions += ZERO,

            Instruction::Add(offset) => {
                instructions += "\tmovzbl (%r12), %eax\n";
                instructions += &format!("\taddb %al, {offset}(%r12)\n");
            },

            Instruction::Sub(offset) => {
                instructions += "\tmovzbl (%r12), %eax\n";
                instructions += &format!("\tsubb %al, {offset}(%r12)\n");
            },

            Instruction::Scan(x) => {
                // Generate label names.
                let label_num = curr_label_num;
                curr_label_num += 1;

                let loop_label = ".SCAN".to_owned() + &label_num.to_string();

                // Generate indices for scan.

                let global_name = "_ymm@indices".to_owned() + &x.to_string();

                if !generated_indices.contains(x) {
                    globals += &("\t.globl\t".to_owned() + &global_name + "\n");
                    globals += &("\t.section	.rdata,\"dr\",discard,".to_owned() + &global_name + "\n");
                    globals += &("\t.p2align	5, 0x0\n");
                    globals += &(global_name.to_owned() + ":\n");
                    for i in 0..8 {
                        globals += &("\t.long\t".to_owned() + &((i * *x).to_string()) + "\n");
                    }

                    generated_indices.insert(*x);
                }

                let bytes_per_iter = 8 * *x;

                instructions += "	movq	%r12, %rax\n";
                instructions += &("	vmovdqa	".to_owned() + &global_name + "(%rip), %ymm0\n");
                instructions += "	vpxor	%xmm1, %xmm1, %xmm1\n";
                instructions += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2\n";
                instructions += &("	movl	$".to_owned() + &bytes_per_iter.to_string() + ", %edx\n");
                instructions += "	.p2align	4, 0x90\n";
                instructions += &(loop_label.to_owned() + ":                                # =>This Inner Loop Header: Depth=1\n");
                instructions += "	vpcmpeqd	%ymm3, %ymm3, %ymm3\n";
                instructions += "	vpxor	%xmm4, %xmm4, %xmm4\n";
                instructions += "	vpgatherdd	%ymm3, (%rax,%ymm0), %ymm4\n";
                instructions += "	vpor	%ymm2, %ymm4, %ymm3\n";
                instructions += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
                instructions += "	vpmovmskb	%ymm3, %r8d\n";
                instructions += "	tzcntl	%r8d, %r9d\n";
                instructions += "	shrl	$2, %r9d\n";
	            instructions += &("	imull	$".to_owned() + &x.to_string() + ", %r9d, %r9d\n");
                instructions += "	addq	%rax, %r9\n";
                instructions += &("	addq	$".to_owned() + &bytes_per_iter.to_string() + ", %rax\n");
                instructions += "	movq	%r9, %r12\n";
                instructions += "	testl	%r8d, %r8d\n";
                instructions += &("	je	".to_owned() + &loop_label + "\n");
                instructions += "# %bb.2:\n";
                instructions += "	vzeroupper\n";
            }

            Instruction::Nop => (),

            _ => panic!("unhandled instruction: {}", inst)
        }
    }

    let program = FUNC_BEGIN.to_owned() + &globals + FUNC_PROLOGUE + &instructions + FUNC_END;
    return program;
}

pub fn compile_to_exe( asm : &str, output_path : &str ) -> Result<()> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir().map_err(|e| Box::new(e))?;

    let runner_path = output_dir.path().join("bf_main.c");
    let mut runner_file = File::create(runner_path.clone()).map_err(|e| Box::new(e))?;
    write!(runner_file, "{}", TEST_RUNNER).unwrap();

    let bf_asm_path = output_dir.path().join("bf_program.S");
    let mut bf_asm_file = File::create(bf_asm_path.clone()).map_err(|e| Box::new(e))?;
    write!(bf_asm_file, "{}", asm).unwrap();

    Command::new("clang")
        .arg(runner_path)
        .arg(bf_asm_path)
        .arg("-march=native")
        .arg("-o")
        .arg(output_path.clone())
        .status().expect("Error compiling BF program.");

    return Ok(());
}

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

#[derive(Debug, Clone)]
struct BadExitCode;

impl fmt::Display for BadExitCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "exited wit hbad exit code")
    }
}

impl error::Error for BadExitCode {}

pub fn run( exe_path : &str ) -> Result<()> {
    let status = Command::new("./".to_owned() + exe_path).status().expect("Error executing BF program.");
    if status.success() {
        return Ok(());
    } else {
        return Err(Box::new(BadExitCode));
    }
}

fn compile_and_run_with_input( program : &mut Vec<Instruction>, program_input : &Vec<u8>, do_simplify_loops : bool, do_simplify_scans : bool ) -> Result<Output> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir().map_err(|e| Box::new(e))?;

    let asm = compile_to_asm(program, do_simplify_loops, do_simplify_scans);

    let exe_path = output_dir.path().join("bf.exe");
    compile_to_exe(&asm, exe_path.to_str().unwrap()).expect("failed to compile program");

    let cmd = Command::new(exe_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn().expect("Error starting BF program.");
    cmd.stdin.as_ref().unwrap().write_all(program_input).unwrap();
    // write!(cmd.stdin.as_ref().unwrap(), "{}", program_input).unwrap();
    
    let output = cmd.wait_with_output().expect("Error running BF program.");
    return Ok(output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::*;
    use std::io::Read;
    use rand::*;

    #[test]
    fn test_execute_empty() {
        let mut input = Vec::new();
        input.write("".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(""), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_read_write_char() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_increment() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",+."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("1").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_decrement() {
        let mut input = Vec::new();
        input.write("1".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",-."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("0").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_right() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_left() {
        let mut input = Vec::new();
        input.write("AB".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>,<."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        assert!(output.find("B").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_take_jump() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>[<.>]"), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_dont_take_jump() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>+[<.>-]"), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_loop() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>+++++[<+>-]<."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_inner_loop() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>+++[>++[<<+>>-]<-]<."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();

        assert!(output.find("6").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_multiple_loops() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_with_input(&mut lex(",>+++[<+>-]++[<+>-]<."), &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_decrement_loop_to_zero() {
        let mut prog = lex("[-]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [Instruction::Zero, Instruction::Nop, Instruction::Nop]);
    }

    #[test]
    fn test_execute_decrement_loop_to_zero() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[-].");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_increment_loop_to_zero() {
        let mut prog = lex("[+]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [Instruction::Zero, Instruction::Nop, Instruction::Nop]);
    }

    #[test]
    fn test_execute_increment_loop_to_zero() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[+].");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_decrement_loop_add_1() {
        let mut prog = lex("[->+<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Add(1),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_execute_decrement_loop_add_1() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[->+<]>.");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 6);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_decrement_loop_sub_1() {
        let mut prog = lex("[->-<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Sub(1),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_execute_decrement_loop_sub_1() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[->-<]>.");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 250);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_increment_loop_add_1() {
        let mut prog = lex("[+>+<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Sub(1),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_increment_loop_add_1_write() {
        let mut prog = lex("[+>+>.<<]");
        let prog_orig = prog.clone();
        simplify_loops(&mut prog);

        assert_eq!(prog, prog_orig);
    }

    #[test]
    fn test_increment_loop_add_1_read() {
        let mut prog = lex("[+>+>,<<]");
        let prog_orig = prog.clone();
        simplify_loops(&mut prog);

        assert_eq!(prog, prog_orig);
    }

    #[test]
    fn test_increment_loop_sub_1() {
        let mut prog = lex("[+>-<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Add(1),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_decrement_loop_add_sub() {
        let mut prog = lex("[->>+++>>>>>-+---<++<<<<<<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Add(2),
            Instruction::Add(2),
            Instruction::Add(2),
            Instruction::Add(6),
            Instruction::Add(6),
            Instruction::Sub(7),
            Instruction::Sub(7),
            Instruction::Sub(7),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_increment_loop_add_sub() {
        let mut prog = lex("[+>>+++>>>>>-+---<++<<<<<<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Sub(2),
            Instruction::Sub(2),
            Instruction::Sub(2),
            Instruction::Sub(6),
            Instruction::Sub(6),
            Instruction::Add(7),
            Instruction::Add(7),
            Instruction::Add(7),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_decrement_loop_nested_add_1() {
        let mut prog = lex("++[->+++[->+<]<]");
        simplify_loops(&mut prog);

        assert_eq!(prog, [
            Instruction::Increment,
            Instruction::Increment,
            Instruction::JumpIfZero,
            Instruction::Decrement,
            Instruction::MoveRight,
            Instruction::Increment,
            Instruction::Increment,
            Instruction::Increment,
            Instruction::Add(1),
            Instruction::Zero,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::MoveLeft,
            Instruction::JumpUnlessZero
        ]);
    }

    #[test]
    fn test_execute_decrement_loop_nested_add_1() {
        let mut input = Vec::new();

        let mut prog = lex("++[->+++[->+<]<]>>.");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 6);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_scan_loop() {
        let mut prog = lex("[>]");
        vectorize_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(1),
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_scan_loop_2() {
        let mut prog = lex("[>>]");
        vectorize_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(2),
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_execute_scan_loop() {
        let mut input = Vec::new();

        let mut prog = lex("+>++>+++>++++>+++++>>++++++<<<<<<[>]>.");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 6);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_scan_loop_2() {
        let mut input = Vec::new();

        let mut prog = lex(">+>>++>>+++>>>++++>+++++<<<<<<<<[>>]>.");

        let run_res = compile_and_run_with_input(&mut prog, &input, true, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 4);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_scan_loop_random() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);

        let num_tests = 10;
        let num_cells = 10;
        let max_scan = 3;

        for _ in 0..num_tests {
            let mut input_prog = Vec::new();

            // Select a random cell to hold cell
            let rand_cell_dist = rand::distributions::Uniform::new(0, num_cells);
            let cell_idx = rng.sample(rand_cell_dist);

            // Generate a random input number for each cell.
            let scan_dist_cells = rand::distributions::Uniform::new(1, 255);
            for cell in 0..num_cells {
                if cell != cell_idx {
                    let cell_value = rng.sample(scan_dist_cells);
                    for j in 0..cell_value {
                        input_prog.push("+");
                    }
                }

                input_prog.push(">");
            }

            // Reset head
            for i in 0..num_cells {
                input_prog.push("<");
            }

            // Generate scan loop
            // TODO(Kincaid): Add support for negative head deltas
            let scan_dist = rand::distributions::Uniform::new(0, max_scan);
            let mut scan_num_skipped = rng.sample(scan_dist);
            if scan_num_skipped == 0 {
                scan_num_skipped += 1;
            }
            input_prog.push("[");
            for _ in 0..scan_num_skipped {
                input_prog.push(">");
            }
            input_prog.push("]>.");

            let mut prog = lex(&(input_prog.join("")));
            //println!("{}", input.join(""));
            //assert!(false);

            let mut input = Vec::new();
            
            // Execute without vectorizing scans
            let no_vec_run_res = compile_and_run_with_input(&mut prog, &input, false, false).unwrap();
            assert!(no_vec_run_res.status.success());

            let no_vec_output = no_vec_run_res.stdout;
            assert_eq!(no_vec_output.len(), 1);
            let no_vec_res = no_vec_output[0];

            let no_vec_err_output = String::from_utf8(no_vec_run_res.stderr).unwrap();
            assert!(no_vec_err_output.find("Exited successfully").is_some());

            // Execute with vectorized scans
            let with_vec_run_res = compile_and_run_with_input(&mut prog, &input, false, true).unwrap();
            assert!(with_vec_run_res.status.success());

            let with_vec_output = with_vec_run_res.stdout;
            assert_eq!(with_vec_output.len(), 1);
            let with_vec_res = with_vec_output[0];

            let with_vec_err_output = String::from_utf8(with_vec_run_res.stderr).unwrap();
            assert!(with_vec_err_output.find("Exited successfully").is_some());

            assert_eq!(with_vec_res, no_vec_res);
        }
    }

    #[test]
    fn test_scan_loop_non_power_2() {
        let mut prog = lex("[>>>]");
        vectorize_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(3),
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
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

            let input_prog = std::fs::read_to_string(prog_path.clone()).expect("unable to read file");
            let mut input = input.clone();

            let run_res = compile_and_run_with_input(&mut lex(&input_prog), &input, true, true).unwrap();
            assert!(run_res.status.success());

            let mut orig_output = Vec::new();
            let mut output_file = File::open(output_path).unwrap();
            output_file.read_to_end(&mut orig_output).unwrap();

            println!("{}", prog_path.to_str().unwrap());
            assert_eq!(run_res.stdout, orig_output);

            let err_output = String::from_utf8(run_res.stderr).unwrap();
            assert!(err_output.find("Exited successfully").is_some());
        }
    }

}
