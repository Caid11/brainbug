# BrainBug

A Rust implementation of a BrainFuck compiler.

Based on this specification: https://github.com/sunjay/brainfuck/blob/master/brainfuck.md 

## How to build

- Install Rust
- Run `cargo build` (use `-r` for release mode)
- Built executable will be in the `target` directory.

## How to run

NOTE: Currently only tested on Windows.

```
Usage: brainbug interp [path to bf file] [options]
       brainbug compile [path to bf file] [options]
Options: -p  Print profile data (interp only)
         -t  Print execution time
         -r  execute compiled binary (compile only)
         -S  compile to asm instead of exe (compile only)
```
