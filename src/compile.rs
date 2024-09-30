use tempfile::{tempfile, NamedTempFile};
use std::error;
use std::io::{Write};
use std::fs::{File};
use std::process::{Command, Stdio, Output};
use std::fmt;

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

pub fn compile_to_asm( input : &Vec<Instruction> ) -> String {
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
            }
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

fn compile_and_run_with_input( input : &str, program_input : &str ) -> Result<Output> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir().map_err(|e| Box::new(e))?;

    let program = lex(&input);
    let asm = compile_to_asm(&program);

    let exe_path = output_dir.path().join("bf.exe");
    compile_to_exe(&asm, exe_path.to_str().unwrap()).expect("failed to compile program");

    let cmd = Command::new(exe_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().expect("Error starting BF program.");
    write!(cmd.stdin.as_ref().unwrap(), "{}", program_input).unwrap();
    
    let output = cmd.wait_with_output().expect("Error running BF program.");
    return Ok(output);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_empty() {
        let run_res = compile_and_run_with_input("", "").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_read_write_char() {
        let run_res = compile_and_run_with_input(",.", "A").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_increment() {
        let run_res = compile_and_run_with_input(",+.", "0").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("1").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_decrement() {
        let run_res = compile_and_run_with_input(",-.", "1").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("0").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_right() {
        let run_res = compile_and_run_with_input(",>.", "A").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_left() {
        let run_res = compile_and_run_with_input(",>,<.", "AB").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        assert!(output.find("B").is_none());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_take_jump() {
        let run_res = compile_and_run_with_input(",>[<.>]", "A").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_dont_take_jump() {
        let run_res = compile_and_run_with_input(",>+[<.>-]", "A").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_loop() {
        let run_res = compile_and_run_with_input(",>+++++[<+>-]<.", "0").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_inner_loop() {
        let run_res = compile_and_run_with_input(",>+++[>++[<<+>>-]<-]<.", "0").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();

        assert!(output.find("6").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_multiple_loops() {
        let run_res = compile_and_run_with_input(",>+++[<+>-]++[<+>-]<.", "0").unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        assert!(output.find("Exited successfully").is_some());
    }

}
