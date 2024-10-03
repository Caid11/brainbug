use tempfile::{tempfile, NamedTempFile};
use core::panic;
use std::error;
use std::io::{Write};
use std::fs::{File};
use std::process::{Command, Stdio, Output};
use std::fmt;
use std::collections::HashMap;

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

	.globl	__ymm@0000000600000007000000040000000500000002000000030000000000000001 # -- Begin function bf_main
	.section	.rdata,\"dr\",discard,__ymm@0000000600000007000000040000000500000002000000030000000000000001
	.p2align	5, 0x0
__ymm@0000000600000007000000040000000500000002000000030000000000000001:
	.long	1                               # 0x1
	.long	0                               # 0x0
	.long	3                               # 0x3
	.long	2                               # 0x2
	.long	5                               # 0x5
	.long	4                               # 0x4
	.long	7                               # 0x7
	.long	6                               # 0x6
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

fn simplify_scans( program : &mut Vec<Instruction>) {
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

                    // TODO(Kincaid): Eventually handle power of twos
                    if head_delta != 1 && head_delta != 2 {
                        continue;
                    }

                    for i in start_pc..(pc + 1) {
                        program[i] = Instruction::Nop;
                    }

                    let scan_power = head_delta.ilog2();
                    program[start_pc] = Instruction::Scan(scan_power.try_into().unwrap());
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
        simplify_scans(input);
    }

    let mut program = FUNC_BEGIN.to_owned();

    let mut curr_label_num = 0;
    let mut label_stack = vec![0;0];

    for inst in input {
        match inst {
            Instruction::MoveRight => program += MOVE_RIGHT,
            Instruction::MoveLeft => program += MOVE_LEFT,
            Instruction::Increment => program += INCREMENT,
            Instruction::Decrement => program += DECREMENT,
            Instruction::Read => program += READ_CHAR,
            Instruction::Write => program += WRITE_CHAR,

            Instruction::JumpIfZero => {
                let new_label_num = curr_label_num;
                curr_label_num += 1;
                label_stack.push(new_label_num);

                program += "\n";

                // Generate a jump to the end label.
                program += "\tcmpb $0, (%r12)\n";
                program += &("\tje .UZ".to_owned() + &new_label_num.to_string() + "\n");

                // Generate a label so corresponding jump unless zero can jump back.
                program += &(".IZ".to_owned() + &new_label_num.to_string() + ":\n");
            },

            Instruction::JumpUnlessZero => {
                program += "\n";

                // Get the current brace from the stack.
                let label_num = label_stack.pop().unwrap();

                // Generate a jump to the start label.
                program += "\tcmpb $0, (%r12)\n";
                program += &("\tjne .IZ".to_owned() + &label_num.to_string() + "\n");

                // Generate a label so corresponding jump if zero can jump back.
                program += &(".UZ".to_owned() + &label_num.to_string() + ":\n");
            },

            Instruction::Zero => program += ZERO,

            Instruction::Add(offset) => {
                program += "\tmovzbl (%r12), %eax\n";
                program += &format!("\taddb %al, {offset}(%r12)\n");
            },

            Instruction::Sub(offset) => {
                program += "\tmovzbl (%r12), %eax\n";
                program += &format!("\tsubb %al, {offset}(%r12)\n");
            },

            Instruction::Scan(x) => {
                if *x != 0 && *x != 1 {
                    panic!("scan power not handled: {x}");
                }

                // Generate label names.
                let label_num = curr_label_num;
                curr_label_num += 1;

                let start_label = ".SS".to_owned() + &label_num.to_string();
                let end_label = ".SE".to_owned() + &label_num.to_string();

                program += "\tmovdqu	(%r12), %xmm1\n";
                program += "\tpxor	%xmm0, %xmm0\n";
                program += "\tpcmpeqb	%xmm0, %xmm1\n";
                program += "\tpmovmskb	%xmm1, %ecx\n";

                if *x == 1 {
                    program += "\tandl	$21845, %ecx\n";
                } else {
                    program += "\ttestl	%ecx, %ecx\n";
                }

                program += &("\tjne	".to_owned() + &end_label + "\n");
                program += "\t.p2align	4, 0x90\n";
                program += &(start_label.clone() + ":\n");
                program += "\tmovdqu	8(%r12), %xmm1\n";
                program += "\taddq	$8, %r12\n";
                program += "\tpcmpeqb	%xmm0, %xmm1\n";
                program += "\tpmovmskb	%xmm1, %ecx\n";

                if *x == 1 {
                    program += "\tandl	$21845, %ecx\n";
                } else {
                    program += "\ttestl	%ecx, %ecx\n";
                }

                program += &("\tje	".to_owned() + &start_label + "\n");
                program += &(end_label + ":\n");
                program += "\trep		bsfl	%ecx, %ecx\n";
                program += "\taddq	%rcx, %r12\n";

                // program += "	movq	(%rcx), %rax\n";
                // program += "	vmovdqa	__ymm@0000000600000007000000040000000500000002000000030000000000000001(%rip), %ymm0 # ymm0 = [1,0,3,2,5,4,7,6]\n";
                // program += "	vpxor	%xmm1, %xmm1, %xmm1\n";
                // program += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2 # ymm2 = [0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255]\n";
                // program += "	movl	$8, %edx\n";
                // program += "	.p2align	4, 0x90\n";
                // program += ".LBB0_1:                                # =>This Inner Loop Header: Depth=1\n";
                // program += "	vpcmpeqd	%ymm3, %ymm3, %ymm3\n";
                // program += "	vpxor	%xmm4, %xmm4, %xmm4\n";
                // program += "	vpgatherdd	%ymm3, (%rax,%ymm0), %ymm4\n";
                // program += "	vpor	%ymm2, %ymm4, %ymm3\n";
                // program += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
                // program += "	vpmovmskb	%ymm3, %r8d\n";
                // program += "	tzcntl	%r8d, %r9d\n";
                // program += "	shrl	$2, %r9d\n";
                // program += "	incl	%r9d\n";
                // program += "	testl	%r8d, %r8d\n";
                // program += "	cmovel	%edx, %r9d\n";
                // program += "	addq	%rax, %r9\n";
                // program += "	addq	$8, %rax\n";
                // program += "	movq	%r9, (%rcx)\n";
                // program += "	testl	%r8d, %r8d\n";
                // program += "	je	.LBB0_1\n";
                // program += "# %bb.2:\n";
                // program += "	vzeroupper\n";

                //program += "	movq	%r12, %rax\n";
                //program += "	vmovdqa	__ymm@0000000600000007000000040000000500000002000000030000000000000001(%rip), %ymm0 # ymm0 = [1,0,3,2,5,4,7,6]\n";
                //program += "	vpxor	%xmm1, %xmm1, %xmm1\n";
                //program += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2 # ymm2 = [0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255,0,255,255,255]\n";
                //program += "	movl	$8, %ecx\n";
                //program += "	.p2align	4, 0x90\n";
                //program += ".LBB0_1:                                # =>This Inner Loop Header: Depth=1\n";
                //program += "	vpxor	%xmm3, %xmm3, %xmm3\n";
                //program += "	vpcmpeqd	%ymm4, %ymm4, %ymm4\n";
                //program += "	vpgatherdd	%ymm4, (%rax,%ymm0), %ymm3\n";
                //program += "	vpor	%ymm2, %ymm3, %ymm3\n";
                //program += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
                //program += "	vpmovmskb	%ymm3, %edx\n";
                //program += "	movl	%edx, %r8d\n";
                //program += "	notl	%r8d\n";
                //program += "	andl	$286331153, %r8d                # imm = 0x11111111\n";
                //program += "	popcntl	%r8d, %r8d\n";
                //program += "	testl	%edx, %edx\n";
                //program += "	cmovel	%ecx, %r8d\n";
                //program += "	addq	%rax, %r8\n";
                //program += "	addq	$8, %rax\n";
                //program += "	movq	%r8, %r12\n";
                //program += "	testl	%edx, %edx\n";
                //program += "	je	.LBB0_1\n";
                //program += "# %bb.2:\n";
                //program += "	vzeroupper\n";
            }

            Instruction::Nop => (),

            _ => panic!("unhandled instruction: {}", inst)
        }
    }

    program += FUNC_END;
    return program.to_string();
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
        simplify_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(0),
            Instruction::Nop,
            Instruction::Nop,
        ]);
    }

    #[test]
    fn test_scan_loop_2() {
        let mut prog = lex("[>>]");
        simplify_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(1),
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
    fn test_scan_loop_non_power_2() {
        let mut prog = lex("[>>>]");
        simplify_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::JumpIfZero,
            Instruction::MoveRight,
            Instruction::MoveRight,
            Instruction::MoveRight,
            Instruction::JumpUnlessZero,
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
