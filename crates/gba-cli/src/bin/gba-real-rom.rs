use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping, Mode};
use gba_runtime::validate_header;

const CARTRIDGE_BASE: u32 = 0x0800_0000;
const DEFAULT_MAX_STEPS: u64 = 512;

#[derive(Debug)]
struct Args {
    rom: PathBuf,
    max_steps: u64,
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut rom = None;
    let mut max_steps = DEFAULT_MAX_STEPS;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-steps" => {
                let value = args.next().ok_or("--max-steps requires a value")?;
                max_steps = value.parse()?;
                if max_steps == 0 {
                    return Err("--max-steps must be greater than zero".into());
                }
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}").into());
            }
            value if rom.is_none() => rom = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }

    Ok(Args {
        rom: rom.ok_or("missing ROM path")?,
        max_steps,
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root")
}

fn mapping(size: usize) -> ImageMapping {
    ImageMapping::new(
        ImageKind::CartridgeRom,
        CARTRIDGE_BASE,
        size as u32,
        CARTRIDGE_BASE,
        Mode::Arm,
    )
}

fn unique_runner_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    env::temp_dir().join(format!(
        "gba-real-rom-runner-{stamp}-{}",
        std::process::id()
    ))
}

fn write_runner(root: &Path, generated: &str) -> Result<(), Box<dyn std::error::Error>> {
    let src = root.join("src");
    fs::create_dir_all(&src)?;
    let runtime_path = workspace_root()
        .join("crates/gba-runtime")
        .canonicalize()?;

    fs::write(src.join("gba_generated.rs"), generated)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gba-real-rom-runner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )?;
    fs::write(
        src.join("main.rs"),
        r#"mod gba_generated;

use std::{env, fs};

use gba_runtime::{Cartridge, CpuMode, GeneratedExecutionExit, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rom_path = env::args().nth(1).ok_or("missing ROM path")?;
    let max_steps: u64 = env::args()
        .nth(2)
        .ok_or("missing step limit")?
        .parse()?;
    let rom = fs::read(rom_path)?;

    let mut runtime = Runtime::new();
    runtime.load_cartridge(Cartridge::from_rom(rom, "saves"));

    if runtime.cpu.mode() != CpuMode::System
        || runtime.cpu.thumb
        || runtime.cpu.cpsr != CpuMode::System as u32
        || runtime.cpu.r[13] != 0x0300_7f00
        || runtime.cpu.r[15] != 0x0800_0000
    {
        return Err("cartridge boot state is not BIOS-compatible".into());
    }

    let result = gba_generated::gba_entry_with_limit(&mut runtime, max_steps)?;
    if result.state.pc() % if result.state.thumb { 2 } else { 4 } != 0 {
        return Err("generated execution produced a misaligned architectural PC".into());
    }

    println!("steps={}", result.steps);
    println!("pc={:#010x}", result.state.pc());
    println!("thumb={}", result.state.thumb);
    println!("sp={:#010x}", result.state.registers[13]);
    println!("cpsr={:#010x}", result.state.cpsr);
    println!("cycles={}", result.state.cycles);
    println!("exit={:?}", result.exit);

    match result.exit {
        GeneratedExecutionExit::Returned { .. } | GeneratedExecutionExit::Halted { .. } => Ok(()),
        GeneratedExecutionExit::StepLimitExceeded { address, .. } => {
            if result.steps != max_steps {
                return Err("step-limit accounting is inconsistent".into());
            }
            if address == 0x0800_0000 {
                return Err("execution never progressed beyond cartridge entry".into());
            }
            if result.state.cycles == 0 {
                return Err("execution made no timing progress".into());
            }
            Ok(())
        }
        GeneratedExecutionExit::ExceptionVector { kind, address, .. } => Err(format!(
            "execution escaped the linked ROM CFG into {kind:?} vector at {address:#010x}"
        )
        .into()),
    }
}
"#,
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let rom = fs::read(&args.rom)?;
    let header = validate_header(&rom)?;
    let image_mapping = mapping(rom.len());
    let program = analyze_with_mapping(&rom, image_mapping)?;
    let generated = generate(&program, "gba_entry");

    println!("real-ROM: {}", args.rom.display());
    println!("real-ROM title: {:?}", header.title);
    println!("real-ROM game-code: {}", header.game_code);
    println!("real-ROM entry target: {:#010x}", header.entry_target);
    println!("real-ROM CFG blocks: {}", program.cfg.blocks.len());
    println!(
        "real-ROM CFG instructions: {}",
        program
            .cfg
            .blocks
            .iter()
            .map(|block| block.instructions.len())
            .sum::<usize>()
    );
    println!("real-ROM max steps: {}", args.max_steps);

    let root = unique_runner_dir();
    let run_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        write_runner(&root, &generated.source)?;
        let output = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(root.join("Cargo.toml"))
            .arg("--")
            .arg(&args.rom)
            .arg(args.max_steps.to_string())
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        print!("{stdout}");
        eprint!("{stderr}");
        if !output.status.success() {
            return Err(format!("real-ROM runner exited with {}", output.status).into());
        }
        Ok(())
    })();

    let _ = fs::remove_dir_all(&root);
    run_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_real_rom_at_cartridge_base() {
        let mapping = mapping(0x200);
        assert_eq!(mapping.kind, ImageKind::CartridgeRom);
        assert_eq!(mapping.base, CARTRIDGE_BASE);
        assert_eq!(mapping.entry, CARTRIDGE_BASE);
        assert_eq!(mapping.entry_mode, Mode::Arm);
    }
}
