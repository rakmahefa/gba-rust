use std::{env, fs};

use gba_recompiler::{analyze, generate, Mode, ROM_BASE};
use gba_runtime::{Cartridge, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rom_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba".into());
    let rom = fs::read(&rom_path)?;
    let program = analyze(&rom, ROM_BASE, Mode::Arm)?;
    println!(
        "static analysis: entry={:#x}, blocks={}, instructions={}",
        program.cfg.blocks[program.entry.0].key.address,
        program.cfg.blocks.len(),
        program
            .cfg
            .blocks
            .iter()
            .map(|b| b.instructions.len())
            .sum::<usize>()
    );

    let generated = generate(&program, "gba_entry");
    let generated_path = "target/gba_generated.rs";
    fs::create_dir_all("target")?;
    fs::write(generated_path, generated.source)?;
    println!("generated Rust: {generated_path}");

    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(rom, "saves"));
    println!(
        "runtime ready: pc={:#x}, rom={} bytes",
        runtime.cpu.r[15],
        runtime.cartridge.as_ref().unwrap().rom.len()
    );
    Ok(())
}
