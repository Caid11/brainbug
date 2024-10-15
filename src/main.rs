use std::{collections::{HashMap, VecDeque}, env, fs, io::{self, Read, Write}, process::ExitCode, time::SystemTime, usize};
use std::path::Path;
use std::fs::File;

mod common;
mod compile;
mod interp;

fn print_usage() {
    println!("Usage: brainbug interp [path to bf file] [options]");
    println!("       brainbug compile [path to bf file] [options]");
    println!("Options: -p                  Print profile data (interp only)");
    println!("         -t                  Print execution time");
    println!("         -r                  execute compiled binary (compile only)");
    println!("         -S                  compile to asm instead of exe (compile only)");
    println!("         -no-loop-simplify   compile to asm instead of exe (compile only)");
    println!("         -no-scan-vectorize  compile to asm instead of exe (compile only)");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let mut mode = "";
    let mut file_path = "";
    let mut profile = false;
    let mut time = false;
    let mut run = false;
    let mut compile_to_asm = false;
    let mut simplify_loops = true;
    let mut vectorize_scans = true;
    let mut partial_eval = false;

    for i in 1..args.len() {
        // Flag arguments
        if args[i] == "-p" {
            profile = true;
            continue;
        } else if args[i] == "-t" {
            time = true;
            continue;
        } else if args[i] == "-r" {
            run = true;
            continue;
        } else if args[i] == "-S" {
            compile_to_asm = true;
            continue;
        } else if args[i] == "-no-loop-simplify" {
            simplify_loops = false;
            continue;
        } else if args[i] == "-no-scan-vectorize" {
            vectorize_scans = false;
            continue;
        } else if args[i] == "-partial-eval" {
            partial_eval = true;
            continue;
        }

        // Positional arguments
        if mode.is_empty() {
            mode = &args[i];
        } else if file_path.is_empty() {
            file_path = &args[i];
        } else {
            print_usage();
            return ExitCode::from(1);
        }
    }

    if mode.is_empty() || file_path.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }
    if profile && mode != "interp" {
        print_usage();
        return ExitCode::from(1);
    }
    if (run || compile_to_asm) && mode != "compile" {
        print_usage();
        return ExitCode::from(1);
    }

    let input = fs::read_to_string(file_path).expect("unable to read file");

    if mode == "interp" {
        let start_time = SystemTime::now();

        let program = common::lex(&input);
        let mut state = interp::State::new(program);
        state.interp(std::io::stdin(), std::io::stdout());

        if time {
            println!("\nExecution time: {}", start_time.elapsed().unwrap().as_secs_f64());
        }

        if profile {
            state.print_profile_info();
        }
    } else if mode == "compile" {
        let mut program = common::lex(&input);
        let compiled_asm = compile::compile_to_asm(&mut program, simplify_loops, vectorize_scans, partial_eval);

        let input_filepath = Path::new(file_path);

        if compile_to_asm {
            let output_filepath = input_filepath.file_stem().unwrap().to_str().unwrap().to_owned() + ".S";
            let mut file = File::create(output_filepath.clone()).expect("Unable to open output file");
            write!(file, "{}", compiled_asm).unwrap();

            println!("Result written to {}", output_filepath);
        } else {
            let output_filepath = input_filepath.file_stem().unwrap().to_str().unwrap().to_owned() + ".exe";
            compile::compile_to_exe(&compiled_asm, &output_filepath).expect("failed to assemble and link compiled asm");
            println!("Result written to {}", output_filepath);

            if run {
                let start_time = SystemTime::now();

                compile::run(&output_filepath).expect("failed to run compiled BF program");

                if time {
                    println!("\nExecution time: {}", start_time.elapsed().unwrap().as_secs_f64());
                }
            }

        }    }

    return ExitCode::from(0);
}
