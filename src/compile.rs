use inkwell::basic_block::BasicBlock;
use inkwell::passes::PassManager;
use inkwell::types::BasicType;
use tempfile::{tempfile, NamedTempFile};
use core::panic;
use std::error;
use std::io::{Write};
use std::fs::{File};
use std::process::{Command, Stdio, Output};
use std::fmt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use inkwell::module::{Linkage, Module};
use inkwell::{targets::*, AddressSpace, IntPredicate, OptimizationLevel};
use inkwell::context::Context;

use crate::common::*;
use crate::interp::State;

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
	movq %rcx, %r13

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

                    // // TODO(Kincaid): Eventually handle left scans
                    // if head_delta < 1 {
                    //     continue;
                    // }

                    if head_delta == 0 {
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

fn partial_eval( program : &mut Vec<Instruction>) {
    let mut state = State::new(program.clone());
    let insts = state.partial_eval();
    *program = insts.clone();
}

pub fn compile_to_asm( input : &mut Vec<Instruction>, do_simplify_loops : bool, do_simplify_scans : bool, do_partial_eval : bool ) -> String {
    if do_partial_eval {
        partial_eval(input);
    }

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

                let is_neg = *x < 0;
                let abs_scan = x.abs();

                let head_delta_str = if !is_neg {
                    &x.to_string()
                } else {
                    &("neg".to_owned() + &abs_scan.to_string())
                };
                let global_name = "_ymm@indices".to_owned() + &head_delta_str;

                if !generated_indices.contains(x) {
                    globals += &("\t.globl\t".to_owned() + &global_name + "\n");
                    globals += &("\t.section	.rdata,\"dr\",discard,".to_owned() + &global_name + "\n");
                    globals += &("\t.p2align	5, 0x0\n");
                    globals += &(global_name.to_owned() + ":\n");

                    if !is_neg {
                        for i in 0..8 {
                            globals += &("\t.long\t".to_owned() + &((i * abs_scan).to_string()) + "\n");
                        }
                    } else {
                        for i in (0..8).rev() {
                            globals += &("\t.long\t".to_owned() + &((i * abs_scan).to_string()) + "\n");
                        }
                    }

                    generated_indices.insert(*x);
                }

                let bytes_per_iter = 8 * abs_scan;

                if !is_neg {
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
                    instructions += &("	imull	$".to_owned() + &abs_scan.to_string() + ", %r9d, %r9d\n");
                    instructions += "	addq	%rax, %r9\n";
                    instructions += &("	addq	$".to_owned() + &bytes_per_iter.to_string() + ", %rax\n");
                    instructions += "	movq	%r9, %r12\n";
                    instructions += "	testl	%r8d, %r8d\n";
                    instructions += &("	je	".to_owned() + &loop_label + "\n");
                    instructions += "# %bb.2:\n";
                    instructions += "	vzeroupper\n";
                } else {
                    let start_offset = 7 * abs_scan;

                    instructions += "	movq	%r12, %rax\n";
                    instructions += &("	addq	$-".to_owned() + &start_offset.to_string() + ", %rax\n");
                    instructions += "	movq	%rax, %r12\n";
                    instructions += &("	vmovdqa	".to_owned() + &global_name + "(%rip), %ymm0\n");
                    instructions += "	vpxor	%xmm1, %xmm1, %xmm1\n";
                    instructions += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2\n";
                    instructions += "	.p2align	4, 0x90\n";
                    instructions += &(loop_label.to_owned() + ":                                # =>This Inner Loop Header: Depth=1\n");
                    instructions += "	vpcmpeqd	%ymm3, %ymm3, %ymm3\n";
                    instructions += "	vpxor	%xmm4, %xmm4, %xmm4\n";
                    instructions += "	vpgatherdd	%ymm3, (%rax,%ymm0), %ymm4\n";
                    instructions += "	vpor	%ymm2, %ymm4, %ymm3\n";
                    instructions += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
                    instructions += "	vpmovmskb	%ymm3, %edx\n";
                    instructions += "	tzcntl	%edx, %r8d\n";
                    instructions += "	shrl	$2, %r8d\n";
                    instructions += &("	imull	$".to_owned() + &abs_scan.to_string() + ", %r8d, %r8d\n");
                    instructions += "	movq	%rax, %r9\n";
                    instructions += "	subq	%r8, %r9\n";
                    instructions += &("	addq	$".to_owned() + &start_offset.to_string() + ", %r9\n");
                    instructions += &("	addq	$-".to_owned() + &bytes_per_iter.to_string() + ", %rax\n");
                    instructions += "	testl	%edx, %edx\n";
                    instructions += "	cmovneq	%r9, %rax\n";
                    instructions += "	movq	%rax, %r12\n";
                    instructions += &("	je	".to_owned() + &loop_label + "\n");
                    instructions += "# %bb.2:\n";
                    instructions += "	vzeroupper\n";
                }
            }

            Instruction::Output(x) => {
                instructions += &format!("    movl ${x}, %ecx\n");
                instructions += "	callq putchar\n";
            },

            Instruction::SetHeadPos(x) => {
                instructions += "   movq %r13, %r12\n";
                instructions += &format!("   addq ${x}, %r12\n");
            },

            Instruction::SetCell(pos, val) => {
                instructions += &format!("   movb ${val}, {pos}(%r13)\n");
            }

            Instruction::Nop => (),

            _ => panic!("unhandled instruction: {}", inst)
        }
    }

    let program = FUNC_BEGIN.to_owned() + &globals + FUNC_PROLOGUE + &instructions + FUNC_END;
    return program;
}

pub fn compile_to_llvm<'a>( context : &'a Context, input : &mut Vec<Instruction>, do_simplify_loops : bool ) -> Module<'a> {
    if do_simplify_loops {
        simplify_loops(input);
    }

    let module = context.create_module("bf_main");

    // Add declarations for getchar and putchar

    let getchar_fn_ty = context.i32_type().fn_type(&[], false);
    let putchar_fn_ty = context.i32_type().fn_type(&[context.i32_type().into()], false);

    let getchar_fn = module.add_function("getchar", getchar_fn_ty, None);
    let putchar_fn = module.add_function("putchar", putchar_fn_ty, None);

    // Add a bf_main function.

    let void_ptr_ty = context.ptr_type(AddressSpace::default());
    let func_ty = context.void_type().fn_type(&[void_ptr_ty.into()], false);

    let bf_main_func = module.add_function("bf_main", func_ty, None);

    // Populate function.

    let builder = context.create_builder();

    // Add basic blocks. Create a basic block whenever a branch is possible (jump if zero or jump
    // unless zero).

    let mut basic_blocks : Vec<BasicBlock> = Vec::new();

    let mut curr_block = context.append_basic_block(bf_main_func, "entry");

    // We need a new basic block for every possible branch instruction.
    let mut curr_bb = 0;
    for inst in &mut *input {
        match inst {
            Instruction::JumpIfZero | Instruction::JumpUnlessZero => {
                basic_blocks.push(context.append_basic_block(bf_main_func, &curr_bb.to_string()));
                curr_bb = curr_bb + 1;
            },

            _ => ()
        }
    }
    builder.position_at_end(curr_block);

    // Allocate a single pointer alloca to track the head position.
    let head_pos_ty = bf_main_func.get_first_param().unwrap().get_type();
    let head_pos = builder.build_alloca(head_pos_ty, "head_pos").unwrap();
    builder.build_store(head_pos, bf_main_func.get_first_param().unwrap()).unwrap();

    // Visit BF insts

    let mut bb_jump_back_stack : Vec<BasicBlock> = Vec::new();
    let mut bb_next_stack : Vec<BasicBlock> = Vec::new();

    for inst in input {
        match inst {
            Instruction::Read => {
                // Call getchar
                let read_value_i32 = builder.build_call(getchar_fn, &[], "read_value_i32").unwrap();
                let read_value_i8 = builder.build_int_truncate(read_value_i32.try_as_basic_value().unwrap_left().into_int_value(), context.i8_type(), "read_value_i8").unwrap();

                // Store read value
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                builder.build_store(curr_head_pos.into_pointer_value(), read_value_i8).unwrap();
            },

            Instruction::Write => {
                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();
                let curr_head_val_i32 = builder.build_int_z_extend(curr_head_val_i8.into_int_value(), context.i32_type(), "curr_head_val_i32").unwrap();

                // Call putchar on value.
                builder.build_call(putchar_fn, &[curr_head_val_i32.into()], "putchar_head").unwrap();
            },


            Instruction::Increment => {
                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                // Add 1 to value
                let added_val_i8 = builder.build_int_add(curr_head_val_i8.into_int_value(), context.i8_type().const_int(1, false), "incremented").unwrap();

                // Store value.
                builder.build_store(curr_head_pos.into_pointer_value(), added_val_i8).unwrap();
            },

            Instruction::Add(x) => {
                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                let x_i64 = i64::from(*x);
                let x_u64 = u64::from_ne_bytes(x_i64.to_ne_bytes());

                // Read value at offset.
                let curr_head_pos_int = builder.build_ptr_to_int(curr_head_pos.into_pointer_value(), context.i64_type(), "head_pos_int").unwrap();
                let offset_head_pos_int = builder.build_int_add(curr_head_pos_int, context.i64_type().const_int(x_u64, false), "offset_head_pos_int").unwrap();
                let offset_head_pos = builder.build_int_to_ptr(offset_head_pos_int, head_pos_ty.into_pointer_type(), "offset_head_pos").unwrap();
                let offset_head_val_i8 = builder.build_load(context.i8_type(), offset_head_pos.try_into().unwrap(), "offset_head_val_i8").unwrap();

                // Add values
                let sum = builder.build_int_add(curr_head_val_i8.into_int_value(), offset_head_val_i8.into_int_value(), "sum").unwrap();

                // Store value.
                builder.build_store(offset_head_pos, sum).unwrap();
            }

            Instruction::Sub(x) => {
                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                let x_i64 = i64::from(*x);
                let x_u64 = u64::from_ne_bytes(x_i64.to_ne_bytes());

                // Read value at offset.
                let curr_head_pos_int = builder.build_ptr_to_int(curr_head_pos.into_pointer_value(), context.i64_type(), "head_pos_int").unwrap();
                let offset_head_pos_int = builder.build_int_add(curr_head_pos_int, context.i64_type().const_int(x_u64, false), "offset_head_pos_int").unwrap();
                let offset_head_pos = builder.build_int_to_ptr(offset_head_pos_int, head_pos_ty.into_pointer_type(), "offset_head_pos").unwrap();
                let offset_head_val_i8 = builder.build_load(context.i8_type(), offset_head_pos.try_into().unwrap(), "offset_head_val_i8").unwrap();

                // Sub values
                let sum = builder.build_int_sub(offset_head_val_i8.into_int_value(), curr_head_val_i8.into_int_value(), "sum").unwrap();

                // Store value.
                builder.build_store(offset_head_pos, sum).unwrap();
            }

            Instruction::Zero => {
                // Get head ptr
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();

                // Store 0 to head.
                builder.build_store(curr_head_pos.into_pointer_value(), context.i8_type().const_zero()).unwrap();
            }

            Instruction::Decrement => {
                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                // Sub 1 from value
                let subbed_val_i8 = builder.build_int_sub(curr_head_val_i8.into_int_value(), context.i8_type().const_int(1, false), "decremented").unwrap();

                // Store value.
                builder.build_store(curr_head_pos.into_pointer_value(), subbed_val_i8).unwrap();
            },

            Instruction::MoveRight => {
                // Read curr head ptr
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();

                // Add 1 to ptr
                let curr_head_pos_int = builder.build_ptr_to_int(curr_head_pos.into_pointer_value(), context.i64_type(), "head_pos_int").unwrap();
                let new_head_pos_int = builder.build_int_add(curr_head_pos_int, context.i64_type().const_int(1, false), "new_head_pos_int").unwrap();
                let new_head_pos = builder.build_int_to_ptr(new_head_pos_int, head_pos_ty.into_pointer_type(), "new_head_pos").unwrap();

                // Store result
                builder.build_store(head_pos, new_head_pos).unwrap();
            },

            Instruction::MoveLeft => {
                // Read curr head ptr
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();

                // Sub 1 from ptr
                let curr_head_pos_int = builder.build_ptr_to_int(curr_head_pos.into_pointer_value(), context.i64_type(), "head_pos_int").unwrap();
                let new_head_pos_int = builder.build_int_sub(curr_head_pos_int, context.i64_type().const_int(1, false), "new_head_pos_int").unwrap();
                let new_head_pos = builder.build_int_to_ptr(new_head_pos_int, head_pos_ty.into_pointer_type(), "new_head_pos").unwrap();

                // Store result
                builder.build_store(head_pos, new_head_pos).unwrap();
            },

            Instruction::JumpIfZero => {
                // Get basic blocks that we'll jump to.
                let if_zero = basic_blocks.pop().unwrap();
                let not_zero = basic_blocks.pop().unwrap();

                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                // Compare val to 0.
                let is_zero = builder.build_int_compare(IntPredicate::EQ, curr_head_val_i8.into_int_value(), context.i8_type().const_zero().into(), "is_zero").unwrap();

                // Create a branch instruction.
                builder.build_conditional_branch(is_zero, if_zero, not_zero).unwrap();

                // Push the if and not zero BBs to the jump back stack. We'll target them when we
                // hit the corresponding jump unless zero.
                bb_jump_back_stack.push(not_zero);
                bb_next_stack.push(if_zero);

                // Set curr block to the loop body.
                builder.position_at_end(not_zero);
            },

            Instruction::JumpUnlessZero => {
                // Get basic blocks that we'll jump to.
                let not_zero_bb = bb_jump_back_stack.pop().unwrap();
                let if_zero_bb = bb_next_stack.pop().unwrap();

                // Read value at head.
                let curr_head_pos = builder.build_load(head_pos_ty, head_pos, "curr_head_pos").unwrap();
                let curr_head_val_i8 = builder.build_load(context.i8_type(), curr_head_pos.try_into().unwrap(), "curr_head_val_i8").unwrap();

                // Compare val to 0.
                let not_zero = builder.build_int_compare(IntPredicate::NE, curr_head_val_i8.into_int_value(), context.i8_type().const_zero().into(), "not_zero").unwrap();

                // Create a branch instruction.
                builder.build_conditional_branch(not_zero, not_zero_bb, if_zero_bb).unwrap();

                // Set curr block to the next block.
                builder.position_at_end(if_zero_bb);
            }

            Instruction::Nop => (),

            _ => panic!("unhandled instruction: {}", inst)
        }
    }

    builder.build_return(None).unwrap();

    // module.write_bitcode_to_path(Path::new("bf_program_bleh.bc"));

    module.verify().unwrap();

    return module;

    // let mut curr_label_num = 0;
    // let mut label_stack = vec![0;0];

    // let mut generated_indices : HashSet<i32> = HashSet::new();

    // for inst in input {
    //     match inst {
    //         Instruction::MoveRight => instructions += MOVE_RIGHT,
    //         Instruction::MoveLeft => instructions += MOVE_LEFT,
    //         Instruction::Increment => instructions += INCREMENT,
    //         Instruction::Decrement => instructions += DECREMENT,
    //         Instruction::Read => instructions += READ_CHAR,
    //         Instruction::Write => instructions += WRITE_CHAR,

    //         Instruction::JumpIfZero => {
    //             let new_label_num = curr_label_num;
    //             curr_label_num += 1;
    //             label_stack.push(new_label_num);

    //             instructions += "\n";

    //             // Generate a jump to the end label.
    //             instructions += "\tcmpb $0, (%r12)\n";
    //             instructions += &("\tje .UZ".to_owned() + &new_label_num.to_string() + "\n");

    //             // Generate a label so corresponding jump unless zero can jump back.
    //             instructions += &(".IZ".to_owned() + &new_label_num.to_string() + ":\n");
    //         },

    //         Instruction::JumpUnlessZero => {
    //             instructions += "\n";

    //             // Get the current brace from the stack.
    //             let label_num = label_stack.pop().unwrap();

    //             // Generate a jump to the start label.
    //             instructions += "\tcmpb $0, (%r12)\n";
    //             instructions += &("\tjne .IZ".to_owned() + &label_num.to_string() + "\n");

    //             // Generate a label so corresponding jump if zero can jump back.
    //             instructions += &(".UZ".to_owned() + &label_num.to_string() + ":\n");
    //         },

    //         Instruction::Zero => instructions += ZERO,

    //         Instruction::Add(offset) => {
    //             instructions += "\tmovzbl (%r12), %eax\n";
    //             instructions += &format!("\taddb %al, {offset}(%r12)\n");
    //         },

    //         Instruction::Sub(offset) => {
    //             instructions += "\tmovzbl (%r12), %eax\n";
    //             instructions += &format!("\tsubb %al, {offset}(%r12)\n");
    //         },

    //         Instruction::Scan(x) => {
    //             // Generate label names.
    //             let label_num = curr_label_num;
    //             curr_label_num += 1;

    //             let loop_label = ".SCAN".to_owned() + &label_num.to_string();

    //             // Generate indices for scan.

    //             let is_neg = *x < 0;
    //             let abs_scan = x.abs();

    //             let head_delta_str = if !is_neg {
    //                 &x.to_string()
    //             } else {
    //                 &("neg".to_owned() + &abs_scan.to_string())
    //             };
    //             let global_name = "_ymm@indices".to_owned() + &head_delta_str;

    //             if !generated_indices.contains(x) {
    //                 globals += &("\t.globl\t".to_owned() + &global_name + "\n");
    //                 globals += &("\t.section	.rdata,\"dr\",discard,".to_owned() + &global_name + "\n");
    //                 globals += &("\t.p2align	5, 0x0\n");
    //                 globals += &(global_name.to_owned() + ":\n");

    //                 if !is_neg {
    //                     for i in 0..8 {
    //                         globals += &("\t.long\t".to_owned() + &((i * abs_scan).to_string()) + "\n");
    //                     }
    //                 } else {
    //                     for i in (0..8).rev() {
    //                         globals += &("\t.long\t".to_owned() + &((i * abs_scan).to_string()) + "\n");
    //                     }
    //                 }

    //                 generated_indices.insert(*x);
    //             }

    //             let bytes_per_iter = 8 * abs_scan;

    //             if !is_neg {
    //                 instructions += "	movq	%r12, %rax\n";
    //                 instructions += &("	vmovdqa	".to_owned() + &global_name + "(%rip), %ymm0\n");
    //                 instructions += "	vpxor	%xmm1, %xmm1, %xmm1\n";
    //                 instructions += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2\n";
    //                 instructions += &("	movl	$".to_owned() + &bytes_per_iter.to_string() + ", %edx\n");
    //                 instructions += "	.p2align	4, 0x90\n";
    //                 instructions += &(loop_label.to_owned() + ":                                # =>This Inner Loop Header: Depth=1\n");
    //                 instructions += "	vpcmpeqd	%ymm3, %ymm3, %ymm3\n";
    //                 instructions += "	vpxor	%xmm4, %xmm4, %xmm4\n";
    //                 instructions += "	vpgatherdd	%ymm3, (%rax,%ymm0), %ymm4\n";
    //                 instructions += "	vpor	%ymm2, %ymm4, %ymm3\n";
    //                 instructions += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
    //                 instructions += "	vpmovmskb	%ymm3, %r8d\n";
    //                 instructions += "	tzcntl	%r8d, %r9d\n";
    //                 instructions += "	shrl	$2, %r9d\n";
    //                 instructions += &("	imull	$".to_owned() + &abs_scan.to_string() + ", %r9d, %r9d\n");
    //                 instructions += "	addq	%rax, %r9\n";
    //                 instructions += &("	addq	$".to_owned() + &bytes_per_iter.to_string() + ", %rax\n");
    //                 instructions += "	movq	%r9, %r12\n";
    //                 instructions += "	testl	%r8d, %r8d\n";
    //                 instructions += &("	je	".to_owned() + &loop_label + "\n");
    //                 instructions += "# %bb.2:\n";
    //                 instructions += "	vzeroupper\n";
    //             } else {
    //                 let start_offset = 7 * abs_scan;

    //                 instructions += "	movq	%r12, %rax\n";
    //                 instructions += &("	addq	$-".to_owned() + &start_offset.to_string() + ", %rax\n");
    //                 instructions += "	movq	%rax, %r12\n";
    //                 instructions += &("	vmovdqa	".to_owned() + &global_name + "(%rip), %ymm0\n");
    //                 instructions += "	vpxor	%xmm1, %xmm1, %xmm1\n";
    //                 instructions += "	vpbroadcastd	__real@ffffff00(%rip), %ymm2\n";
    //                 instructions += "	.p2align	4, 0x90\n";
    //                 instructions += &(loop_label.to_owned() + ":                                # =>This Inner Loop Header: Depth=1\n");
    //                 instructions += "	vpcmpeqd	%ymm3, %ymm3, %ymm3\n";
    //                 instructions += "	vpxor	%xmm4, %xmm4, %xmm4\n";
    //                 instructions += "	vpgatherdd	%ymm3, (%rax,%ymm0), %ymm4\n";
    //                 instructions += "	vpor	%ymm2, %ymm4, %ymm3\n";
    //                 instructions += "	vpcmpeqb	%ymm1, %ymm3, %ymm3\n";
    //                 instructions += "	vpmovmskb	%ymm3, %edx\n";
    //                 instructions += "	tzcntl	%edx, %r8d\n";
    //                 instructions += "	shrl	$2, %r8d\n";
    //                 instructions += &("	imull	$".to_owned() + &abs_scan.to_string() + ", %r8d, %r8d\n");
    //                 instructions += "	movq	%rax, %r9\n";
    //                 instructions += "	subq	%r8, %r9\n";
    //                 instructions += &("	addq	$".to_owned() + &start_offset.to_string() + ", %r9\n");
    //                 instructions += &("	addq	$-".to_owned() + &bytes_per_iter.to_string() + ", %rax\n");
    //                 instructions += "	testl	%edx, %edx\n";
    //                 instructions += "	cmovneq	%r9, %rax\n";
    //                 instructions += "	movq	%rax, %r12\n";
    //                 instructions += &("	je	".to_owned() + &loop_label + "\n");
    //                 instructions += "# %bb.2:\n";
    //                 instructions += "	vzeroupper\n";
    //             }
    //         }

    //         Instruction::Output(x) => {
    //             instructions += &format!("    movl ${x}, %ecx\n");
    //             instructions += "	callq putchar\n";
    //         },

    //         Instruction::SetHeadPos(x) => {
    //             instructions += "   movq %r13, %r12\n";
    //             instructions += &format!("   addq ${x}, %r12\n");
    //         },

    //         Instruction::SetCell(pos, val) => {
    //             instructions += &format!("   movb ${val}, {pos}(%r13)\n");
    //         }

    //         Instruction::Nop => (),

    //         _ => panic!("unhandled instruction: {}", inst)
    //     }
    // }
}

pub fn compile_asm_to_exe( asm : &str, output_path : &str) -> Result<()> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir_in(".").map_err(|e| Box::new(e))?;

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

pub fn compile_llvm_to_exe( module : &Module, output_path : &str, dump_llvm : bool) -> Result<()> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir_in(".").map_err(|e| Box::new(e))?;

    let runner_path = output_dir.path().join("bf_main.c");
    let mut runner_file = File::create(runner_path.clone()).map_err(|e| Box::new(e))?;
    write!(runner_file, "{}", TEST_RUNNER).unwrap();

    // Write the module to an object file

    Target::initialize_all(&InitializationConfig::default());

    let target_triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&target_triple).unwrap();
    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default)
        .unwrap();

    module.set_triple(&target_triple);
    module.set_data_layout(&target_machine.get_target_data().get_data_layout());

    let bf_obj_path = output_dir.path().join("bf_program.o");

    target_machine.write_to_file(module, FileType::Object, &bf_obj_path).unwrap();

    if dump_llvm {
        module.write_bitcode_to_path(Path::new("bf_program.bc"));
    }

    Command::new("clang")
        .arg(runner_path)
        .arg(bf_obj_path)
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

fn compile_and_run_asm_with_input( program : &mut Vec<Instruction>, program_input : &Vec<u8>, do_simplify_loops : bool, do_simplify_scans : bool, do_partial_eval : bool ) -> Result<Output> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir().map_err(|e| Box::new(e))?;

    let asm = compile_to_asm(program, do_simplify_loops, do_simplify_scans, do_partial_eval);

    let exe_path = output_dir.path().join("bf.exe");
    compile_asm_to_exe(&asm, exe_path.to_str().unwrap()).expect("failed to compile program");

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

fn compile_and_run_llvm_with_input( program : &mut Vec<Instruction>, program_input : &Vec<u8>, do_simplify_loops : bool, dump_llvm : bool ) -> Result<Output> {
    let output_dir = tempfile::Builder::new()
        .keep(false)
        .tempdir().map_err(|e| Box::new(e))?;

    let context = Context::create();
    let module = compile_to_llvm(&context, program, do_simplify_loops );

    let exe_path = output_dir.path().join("bf.exe");
    compile_llvm_to_exe(&module, exe_path.to_str().unwrap(), dump_llvm).expect("failed to compile program");

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
    fn test_execute_empty_asm() {
        let mut input = Vec::new();
        input.write("".as_bytes());

        let run_res = compile_and_run_asm_with_input(&mut lex(""), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_empty_llvm() {
        let mut input = Vec::new();
        input.write("".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(""), &input, true, false ).unwrap();
        assert!(run_res.status.success());

        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_read_write_char() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_asm_with_input(&mut lex(",."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_read_write_char_llvm() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",+."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("1").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_increment_llvm() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",+."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",-."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("0").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_decrement_llvm() {
        let mut input = Vec::new();
        input.write("1".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",-."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_right_llvm() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>,<."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        assert!(output.find("B").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_move_left_llvm() {
        let mut input = Vec::new();
        input.write("AB".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>,<."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>[<.>]"), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_none());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_take_jump_llvm() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>[<.>]"), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>+[<.>-]"), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("A").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_jump_if_zero_dont_take_jump_llvm() {
        let mut input = Vec::new();
        input.write("A".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>+[<.>-]"), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>+++++[<+>-]<."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_loop_llvm() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>+++++[<+>-]<."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>+++[>++[<<+>>-]<-]<."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();

        assert!(output.find("6").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_inner_loop_llvm() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>+++[>++[<<+>>-]<-]<."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut lex(",>+++[<+>-]++[<+>-]<."), &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = String::from_utf8(run_res.stdout).unwrap();
        assert!(output.find("5").is_some());
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_multiple_loops_llvm() {
        let mut input = Vec::new();
        input.write("0".as_bytes());

        let run_res = compile_and_run_llvm_with_input(&mut lex(",>+++[<+>-]++[<+>-]<."), &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

#[test]
    fn test_execute_decrement_loop_to_zero_llvm() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[-].");

        let run_res = compile_and_run_llvm_with_input(&mut prog, &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_increment_loop_to_zero_llvm() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[+].");

        let run_res = compile_and_run_llvm_with_input(&mut prog, &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 250);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_decrement_loop_sub_1_llvm() {
        let mut input = Vec::new();

        let mut prog = lex("++++++[->-<]>.");

        let run_res = compile_and_run_llvm_with_input(&mut prog, &input, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 6);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_decrement_loop_nested_add_1_llvm() {
        let mut input = Vec::new();

        let mut prog = lex("++[->+++[->+<]<]>>.");

        let run_res = compile_and_run_llvm_with_input(&mut prog, &input, true, false).unwrap();
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
    fn test_scan_loop_left() {
        let mut prog = lex("[<]");
        vectorize_scans(&mut prog);

        assert_eq!(prog, [
            Instruction::Scan(-1),
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 6);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_scan_loop_left() {
        let mut input = Vec::new();

        let mut prog = lex("+<++<+++<++++<+++++<<++++++>>>>>>[<]<.");

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
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

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 4);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_scan_loop_2_left() {
        let mut input = Vec::new();

        let mut prog = lex("+<<++<<+++<<<++++<+++++>>>>>>>>[<<]<.");

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, true, true, false).unwrap();
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
        let num_cells = 50;
        let max_scan = 20;

        for _ in 0..num_tests {
            let mut input_prog = Vec::new();

            let scan_dist = rand::distributions::Uniform::new(-max_scan, max_scan);
            let mut scan_num_skipped : i32 = rng.sample(scan_dist);
            if scan_num_skipped == 0 {
                scan_num_skipped += 1;
            }

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

                if scan_num_skipped > 0 {
                    input_prog.push(">");
                } else {
                    input_prog.push("<");
                }
            }

            // Reset head
            for i in 0..num_cells {
                if scan_num_skipped > 0 {
                    input_prog.push("<");
                } else {
                    input_prog.push(">");
                }
            }

            // Generate scan loop
            // TODO(Kincaid): Add support for negative head deltas
            input_prog.push("[");
            for _ in 0..(scan_num_skipped.abs()) {
                if scan_num_skipped > 0 {
                    input_prog.push(">");
                } else {
                    input_prog.push("<");
                }
            }
            input_prog.push("]");
            if scan_num_skipped > 0 {
                input_prog.push(">");
            } else {
                input_prog.push("<");
            }
            input_prog.push(".");

            let mut prog = lex(&(input_prog.join("")));
            //println!("{}", input.join(""));
            //assert!(false);

            let mut input = Vec::new();
            
            // Execute without vectorizing scans
            let no_vec_run_res = compile_and_run_asm_with_input(&mut prog, &input, false, false, false).unwrap();
            assert!(no_vec_run_res.status.success());

            let no_vec_output = no_vec_run_res.stdout;
            assert_eq!(no_vec_output.len(), 1);
            let no_vec_res = no_vec_output[0];

            let no_vec_err_output = String::from_utf8(no_vec_run_res.stderr).unwrap();
            assert!(no_vec_err_output.find("Exited successfully").is_some());

            // Execute with vectorized scans
            let with_vec_run_res = compile_and_run_asm_with_input(&mut prog, &input, false, true, false).unwrap();
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
    fn test_execute_partial_eval() {
        let mut input = Vec::new();

        let mut prog = lex("+.>++.>+++.");

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, false, false, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 3);
        assert_eq!(output[0], 1);
        assert_eq!(output[1], 2);
        assert_eq!(output[2], 3);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_partial_eval_one_unknown() {
        let mut input = vec![7;1];

        let mut prog = lex("+++>,<.>.");

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, false, false, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 2);
        assert_eq!(output[0], 3);
        assert_eq!(output[1], 7);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
    }

    #[test]
    fn test_execute_partial_eval_set_head_pos() {
        let mut input = vec![1,2,0,3];

        let mut prog = lex("+[,],.");

        let run_res = compile_and_run_asm_with_input(&mut prog, &input, false, false, true).unwrap();
        assert!(run_res.status.success());

        let output = run_res.stdout;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 3);
        let err_output = String::from_utf8(run_res.stderr).unwrap();
        assert!(err_output.find("Exited successfully").is_some());
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

            let run_res = compile_and_run_asm_with_input(&mut lex(&input_prog), &input, true, true, true).unwrap();
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

    #[test]
    #[ignore]
    fn test_bfcheck_llvm() {
        let (progs, outputs, input_path) = get_tests();

        let mut input_file = File::open(input_path).unwrap();
        let mut input = Vec::new();
        input_file.read_to_end(&mut input).unwrap();

        for i in 0..progs.len() {
            let prog_path = progs[i].clone();
            let output_path = outputs[i].clone();

            let input_prog = std::fs::read_to_string(prog_path.clone()).expect("unable to read file");
            let mut input = input.clone();

            let run_res = compile_and_run_llvm_with_input(&mut lex(&input_prog), &input, true, false).unwrap();
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
