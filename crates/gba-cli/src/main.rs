use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping};
use gba_runtime::{Cartridge, Runtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageArg {
    Rom,
    Bios,
}

impl ImageArg {
    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value {
            "rom" => Ok(Self::Rom),
            "bios" => Ok(Self::Bios),
            other => Err(format!("unknown --image kind: {other} (expected rom or bios)").into()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Rom => "rom",
            Self::Bios => "bios",
        }
    }
}

#[derive(Debug)]
struct Args {
    rom_path: PathBuf,
    execute: bool,
    max_steps: u64,
    image: ImageArg,
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut rom_path = None;
    let mut execute = false;
    let mut max_steps = 100_000;
    let mut image = ImageArg::Rom;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--execute" => execute = true,
            "--max-steps" => {
                max_steps = args.next().ok_or("--max-steps requires a value")?.parse()?;
            }
            "--image" => {
                image = ImageArg::parse(&args.next().ok_or("--image requires rom or bios")?)?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}").into());
            }
            value if rom_path.is_none() => rom_path = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }

    Ok(Args {
        rom_path: rom_path
            .unwrap_or_else(|| PathBuf::from("roms/1636 - Pokemon Fire Red (U)(Squirrels).gba")),
        execute,
        max_steps,
        image,
    })
}

fn image_mapping(image: ImageArg, size: usize) -> ImageMapping {
    match image {
        ImageArg::Rom => ImageMapping::new(
            ImageKind::CartridgeRom,
            0x0800_0000,
            size as u32,
            0x0800_0000,
            gba_recompiler::Mode::Arm,
        ),
        ImageArg::Bios => ImageMapping::new(
            ImageKind::Bios,
            0x0000_0000,
            size as u32,
            0x0000_0000,
            gba_recompiler::Mode::Arm,
        ),
    }
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
    let image_kind = args.next().ok_or("missing image kind")?;
    let image_path = PathBuf::from(args.next().ok_or("missing image path")?);
    let max_steps: u64 = args
        .next()
        .ok_or("missing max step count")?
        .parse()?;

    let image = fs::read(&image_path)?;
    let mut runtime = Runtime::new();

    match image_kind.as_str() {
        "rom" => runtime.load_cartridge(Cartridge::from_rom(image, "saves")),
        "bios" => runtime.load_bios(&image)?,
        other => return Err(format!("unsupported image kind: {other}").into()),
    }

    let result = gba_generated::gba_entry_with_limit(&mut runtime, max_steps)
        .map_err(|error| format!("generated execution failed: {error}"))?;

    println!("generated execution: image={image_kind}");
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

fn execute_generated_image(
    generated_source: &str,
    image_path: &Path,
    image: ImageArg,
    max_steps: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let runner = write_generated_runner(generated_source)?;
    let status = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(runner.join("Cargo.toml"))
        .arg("--")
        .arg(image.as_str())
        .arg(image_path)
        .arg(max_steps.to_string())
        .status()?;

    if !status.success() {
        return Err(format!("generated image runner exited with {status}").into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let image = fs::read(&args.rom_path)?;
    let mapping = image_mapping(args.image, image.len());
    let program = analyze_with_mapping(&image, mapping)?;

    println!(
        "static analysis: image={:?}, entry={:#x}, region={:?}, blocks={}, instructions={}",
        mapping.kind,
        program.cfg.blocks[program.entry.0].key.address,
        program.address_space.region_at(mapping.entry),
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
            "executing generated image: image={:?}, max_steps={}, dispatcher=linked CFG",
            args.image, args.max_steps
        );
        execute_generated_image(
            &generated.source,
            &args.rom_path,
            args.image,
            args.max_steps,
        )?;
        return Ok(());
    }

    if args.image == ImageArg::Rom {
        let mut runtime = Runtime::new();
        runtime.load_cartridge(Cartridge::from_rom(image, "saves"));
        println!(
            "runtime ready: pc={:#x}, rom={} bytes",
            runtime.cpu.r[15],
            runtime.cartridge.as_ref().unwrap().rom.len()
        );
    } else {
        let mut runtime = Runtime::new();
        runtime.load_bios(&image)?;
        println!(
            "runtime ready: BIOS mapped bytes={}, entry=0x0",
            runtime.bios().bytes().len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bios_mapping_keeps_entry_at_zero() {
        let mapping = image_mapping(ImageArg::Bios, 0x4000);
        assert_eq!(mapping.kind, ImageKind::Bios);
        assert_eq!(mapping.base, 0);
        assert_eq!(mapping.entry, 0);
        assert_eq!(mapping.entry_mode, gba_recompiler::Mode::Arm);
    }
}
