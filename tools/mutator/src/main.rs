//! # mutator
//!
//! Benchmark pair mutation utility for Project WARRANT.
//! Generates sound <-> unsound mutant pairs by introducing controlled
//! safety violations (out-of-bounds pointer offsets, unsound Sync/Send impls, uninitialized reads).

use clap::{Parser, ValueEnum};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mutator", about = "Soundness pair mutation utility for benchmark suite")]
struct MutatorCli {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(short, long, value_enum, default_value = "inject-oob")]
    mutation: MutationKind,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum MutationKind {
    /// Mutate safe bounds checking to unchecked indexing
    InjectOob,
    /// Mutate interior mutability struct to declare unsafe impl Sync
    InjectSyncRace,
    /// Invert safe transmute assertions
    InvalidTransmute,
}

fn main() -> anyhow::Result<()> {
    let cli = MutatorCli::parse();
    let source = fs::read_to_string(&cli.input)?;

    let mutated = match cli.mutation {
        MutationKind::InjectOob => {
            if source.contains(".get(") {
                source.replace(".get(", ".get_unchecked(")
            } else if source.contains("[idx]") {
                source.replace("[idx]", "unsafe { *ptr.add(idx) }")
            } else {
                format!("// Mutant: Injected OOB pattern\n{}\npub unsafe fn mutant_oob_access(p: *const u8, o: usize) -> u8 {{ *p.add(o) }}\n", source)
            }
        }
        MutationKind::InjectSyncRace => {
            format!(
                "{}\n\n// Injected unsound Sync implementation\nunsafe impl<T> Sync for InsecureWrapper<T> {{}}\n",
                source
            )
        }
        MutationKind::InvalidTransmute => {
            format!(
                "{}\n\npub unsafe fn mutant_transmute_bool(v: u8) -> bool {{\n    std::mem::transmute(v)\n}}\n",
                source
            )
        }
    };

    fs::write(&cli.output, mutated)?;
    println!("Successfully applied mutation {:?} -> {}", cli.mutation, cli.output.display());
    Ok(())
}
