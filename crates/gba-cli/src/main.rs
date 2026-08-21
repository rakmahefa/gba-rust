use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use gba_recompiler::{analyze, generate, Mode, ROM_BASE};
use gba_runtime::{Cartridge, Runtime};

#[derive(Debug)]
struct Args {
    rom_path: PathBuf,
    execute: bool,
    max_steps: u64,
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut rom_path = None;
    let mut execute = false;
    let mut max_steps = 100_000;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--max-steps" => {
                max_steps = args
                    .next()
                    .ok_or("--max-steps requires a value")?
                    .parse()?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}").into());
            }
            value if rom_path.is_none() => rom_path = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }

    Ok(Args {
        rom_path: rom_path.unwrap_or_else(|| {
            PathBuf::from("roms/1636 - Pokemon Fire Red (U)(Squirrels).gba")
        }),
        execute,
        max_steps,
    })
}

fn workspace_generated_runner_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/gba-generated-runner")
}

fn write_generated_runner(source: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = workspace_generated_runner_dir();
    let src = root.join("src");
    fs::create_dir_all(&src)?;

    fs::write(src.join("gba_generated.rs"), source)?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"gba-generated-runner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = { path = \"../../crates/gba-runtime\" }\n",
    )?;
    fs::write(
        src.join("main.rs"),
        r#"mod gba_generated;

use std::{env, fs, path::PathBuf};

use gba_runtime::{Cartridge, GeneratedExecutionExit, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let rom_path = PathBuf::from(args.next().ok_or("missing ROM path")?);
    let max_steps: u64 = args
        .next()
        .ok_or("missing max step count")?
        .parse()?;

    let rom = fs::read(&rom_path)?;
    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(rom, "saves"));

    let result = gba_generated::gba_entry_with_limit(&mut runtime, max_steps)
        .map_err(|error| format!("generated execution failed: {error}"))?;

    println!("generated execution: steps={}", result.steps);
    println!("generated execution: pc={:#010x}", result.state.pc());
    println!("generated execution: thumb={}", result.state.thumb);
    println!("generated execution: cycles={}", result.state.cycles);

    match result.exit {
        GeneratedExecutionExit::Returned { address, thumb } => {
            println!("generated execution: returned={address:#010x} thumb={thumb}");
        }
        GeneratedExecutionExit::Halted { address, thumb } => {
            println!("generated execution: halted={address:#010x} thumb={thumb}");
        }
        GeneratedExecutionExit::StepLimitExceeded { address, thumb } => {
            println!("generated execution: step_limit={address:#010x} thumb={thumb}");
        }
    }

    Ok(())
}
"#,
    )?;

    Ok(root)
}

fn execute_generated_rom(
    generated_source: &str,
    rom_path: &Path,
    max_steps: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let runner = write_generated_runner(generated_source)?;
    let status = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(runner.join("Cargo.toml"))
        .arg("--")
        .arg(rom_path)
        .arg(max_steps.to_string())
        .status()?;

    if !status.success() {
        return Err(format!("generated ROM runner exited with {status}").into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let rom = fs::read(&args.rom_path)?;
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
    fs::write(generated_path, &generated.source)?;
    println!("generated Rust: {generated_path}");

    if args.execute {
        println!(
            "executing generated ROM: max_steps={}, dispatcher=linked CFG",
            args.max_steps
        );
        execute_generated_rom(&generated.source, &args.rom_path, args.max_steps)?;
        return Ok(());
    }

    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(rom, "saves"));
    println!(
        "runtime ready: pc={:#x}, rom={} bytes",
        runtime.cpu.r[15],
        runtime.cartridge.as_ref().unwrap().rom.len()
    );
    Ok(())
}
