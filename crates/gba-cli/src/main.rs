use std::{env, fs};
use gba_recompiler::{analyze, generate_rust, ROM_BASE};
use gba_runtime::{Cartridge, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rom_path = env::args().nth(1).unwrap_or_else(|| "roms/1636 - Pokemon Fire Red (U)(Squirrels).gba".into());
    let rom = fs::read(&rom_path)?;
    let program = analyze(&rom, ROM_BASE)?;
    println!("static analysis: entry={:#x}, blocks={}", program.entry, program.blocks.len());
    println!("generated Rust:\n{}", generate_rust(&program));
    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(rom, "saves"));
    println!("runtime ready: pc={:#x}, rom={} bytes", runtime.cpu.r[15], runtime.cartridge.as_ref().unwrap().rom.len());
    Ok(())
}
