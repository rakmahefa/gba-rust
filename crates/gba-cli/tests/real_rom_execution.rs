use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping, Mode};
use gba_runtime::{validate_header, Cartridge, Runtime};

const REAL_ROM_ENV: &str = "GBA_REAL_ROM";
const DEFAULT_STEP_LIMIT: u64 = 512;
const BOOT_STEP_LIMIT: u64 = 4096;
const CARTRIDGE_BASE: u32 = 0x0800_0000;
const TRACE_LIMIT: &str = "32";

fn unique_runner_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    env::temp_dir().join(format!(
        "gba-real-rom-validation-{stamp}-{}",
        std::process::id()
    ))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root")
}

fn real_rom_path() -> Option<PathBuf> {
    let configured = env::var_os(REAL_ROM_ENV).map(PathBuf::from)?;
    if configured.is_absolute() {
        return Some(configured);
    }

    let workspace_relative = workspace_root().join(&configured);
    if workspace_relative.exists() {
        Some(workspace_relative)
    } else {
        Some(configured)
    }
}

fn real_rom_mapping(size: usize) -> ImageMapping {
    ImageMapping::new(
        ImageKind::CartridgeRom,
        CARTRIDGE_BASE,
        size as u32,
        CARTRIDGE_BASE,
        Mode::Arm,
    )
}

fn write_generated_runner(root: &Path, source: &str) {
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create generated real-ROM runner");

    let runtime_path = workspace_root()
        .join("crates/gba-runtime")
        .canonicalize()
        .expect("resolve gba-runtime path");

    fs::write(src.join("gba_generated.rs"), source).expect("write generated real-ROM module");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gba-real-rom-validation\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )
    .expect("write temporary Cargo manifest");
    fs::write(
        src.join("main.rs"),
        r#"mod gba_generated;

use std::{env, fs};

use gba_runtime::GeneratedExecutionExit;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rom_path = env::args().nth(1).ok_or("missing ROM path")?;
    let max_steps: u64 = env::args()
        .nth(2)
        .ok_or("missing step limit")?
        .parse()?;

    let rom = fs::read(&rom_path)?;
    let header = gba_runtime::validate_header(&rom)?;
    let mut runtime = gba_runtime::Runtime::new();
    runtime.load_cartridge(gba_runtime::Cartridge::from_rom(rom.clone(), "saves"));
    assert_eq!(
        runtime.cpu.r[13], 0x0300_7f00,
        "cartridge execution must receive the BIOS-compatible system stack"
    );

    let result = gba_generated::gba_entry_with_limit(&mut runtime, max_steps)?;
    assert_eq!(
        result.state.pc() % if result.state.thumb { 2 } else { 4 },
        0,
        "generated execution must preserve architectural PC alignment"
    );

    println!("real-rom identity: size={}", rom.len());
    println!("real-rom identity: title={}", header.title);
    println!("real-rom identity: game_code={}", header.game_code);
    println!("real-rom identity: entry_target={:#010x}", header.entry_target);
    println!("real-rom execution: steps={}", result.steps);
    println!("real-rom execution: pc={:#010x}", result.state.pc());
    println!("real-rom execution: thumb={}", result.state.thumb);
    println!("real-rom execution: sp={:#010x}", result.state.registers[13]);
    println!("real-rom execution: cycles={}", result.state.cycles);
    println!("real-rom execution: exit={:?}", result.exit);

    match result.exit {
        GeneratedExecutionExit::Returned { .. } | GeneratedExecutionExit::Halted { .. } => Ok(()),
        GeneratedExecutionExit::StepLimitExceeded { address, .. } => {
            if result.steps != max_steps {
                return Err("generated execution reported an inconsistent step-limit count".into());
            }
            if address == 0x0800_0000 {
                return Err("real-ROM execution never progressed beyond the cartridge entry".into());
            }
            Ok(())
        }
        GeneratedExecutionExit::ExceptionVector { kind, .. } => {
            Err(format!("execution escaped into an unlinked exception vector: {kind:?}").into())
        }
    }
}
"#,
    )
    .expect("write generated real-ROM runner");
}

fn run_generated_runner(
    root: &Path,
    path: &Path,
    max_steps: u64,
) -> std::process::Output {
    Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .arg("--")
        .arg(path)
        .arg(max_steps.to_string())
        .env("GBA_GENERATED_TRACE", "1")
        .env("GBA_GENERATED_TRACE_LIMIT", TRACE_LIMIT)
        .output()
        .expect("cargo must be available for real-ROM execution validation")
}

fn build_generated_real_rom(path: &Path) -> (PathBuf, String) {
    let rom = fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read real ROM {}: {error}", path.display())
    });
    let header = validate_header(&rom).unwrap_or_else(|error| {
        panic!("real ROM header validation failed for {}: {error}", path.display())
    });

    assert_eq!(header.entry_target & 3, 0);
    assert!(!header.title.trim().is_empty());
    assert_eq!(header.game_code.len(), 4);

    let mapping = real_rom_mapping(rom.len());
    let program = analyze_with_mapping(&rom, mapping).unwrap_or_else(|error| {
        panic!(
            "real ROM CFG analysis failed for {}: {error:?}",
            path.display()
        )
    });

    assert_eq!(program.entry.0, 0);
    assert_eq!(program.cfg.blocks[0].key.address, CARTRIDGE_BASE);
    assert_eq!(program.cfg.blocks[0].key.mode, Mode::Arm);
    assert!(!program.cfg.blocks.is_empty());
    assert!(
        program
            .cfg
            .blocks
            .iter()
            .all(|block| block.key.address >= CARTRIDGE_BASE)
    );

    let generated = generate(&program, "gba_entry");
    assert!(generated.source.contains("GeneratedBlockExit"));
    assert!(generated.source.contains("gba_entry_with_limit"));

    let mut preflight = Runtime::new();
    preflight.load_cartridge(Cartridge::from_rom(rom, "saves"));
    assert_eq!(preflight.cpu.r[15], CARTRIDGE_BASE);
    assert_eq!(preflight.cpu.r[13], 0x0300_7f00);

    let root = unique_runner_dir();
    write_generated_runner(&root, &generated.source);
    (
        root,
        format!(
            "size={} title={} game_code={} entry_target={:#010x}",
            preflight
                .cartridge
                .as_ref()
                .expect("cartridge loaded")
                .rom
                .len(),
            header.title,
            header.game_code,
            header.entry_target
        ),
    )
}

#[test]
fn real_rom_execution_validates_cartridge_cfg_and_runtime_boundary() {
    let Some(path) = real_rom_path() else {
        eprintln!(
            "skipping real-ROM execution validation: set {REAL_ROM_ENV} to a local .gba ROM"
        );
        return;
    };

    let rom = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{REAL_ROM_ENV} points to unreadable ROM {}: {error}",
            path.display()
        )
    });

    let mapping = real_rom_mapping(rom.len());
    let program = analyze_with_mapping(&rom, mapping).unwrap_or_else(|error| {
        panic!(
            "real ROM CFG analysis failed for {}: {error:?}",
            path.display()
        )
    });

    assert_eq!(program.entry.0, 0);
    assert_eq!(program.cfg.blocks[0].key.address, CARTRIDGE_BASE);
    assert_eq!(program.cfg.blocks[0].key.mode, Mode::Arm);
    assert!(!program.cfg.blocks.is_empty());
    assert!(
        program
            .cfg
            .blocks
            .iter()
            .all(|block| block.key.address >= CARTRIDGE_BASE)
    );

    let generated = generate(&program, "gba_entry");
    assert!(generated.source.contains("GeneratedBlockExit"));
    assert!(generated.source.contains("gba_entry_with_limit"));

    let header = validate_header(&rom).unwrap_or_else(|error| {
        panic!(
            "real ROM header validation failed for {}: {error}",
            path.display()
        )
    });
    let mut preflight = Runtime::new();
    preflight.load_cartridge(Cartridge::from_rom(rom, "saves"));
    assert_eq!(preflight.cpu.r[15], CARTRIDGE_BASE);
    assert_eq!(preflight.cpu.r[13], 0x0300_7f00);

    let root = unique_runner_dir();
    write_generated_runner(&root, &generated.source);

    let first = run_generated_runner(&root, &path, DEFAULT_STEP_LIMIT);
    assert!(
        first.status.success(),
        "real-ROM generated runner failed for {}\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_generated_runner(&root, &path, DEFAULT_STEP_LIMIT);
    assert!(
        second.status.success(),
        "second real-ROM generated runner failed for {}\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains("real-rom identity: size="));
    assert!(stdout.contains(&format!("real-rom identity: title={}", header.title)));
    assert!(stdout.contains(&format!("real-rom identity: game_code={}", header.game_code)));
    assert!(stdout.contains("real-rom execution: steps="));
    assert!(stdout.contains("real-rom execution: pc="));
    assert!(stdout.contains("real-rom execution: thumb="));
    assert!(stdout.contains("real-rom execution: sp="));
    assert!(stdout.contains("real-rom execution: cycles="));
    assert!(stdout.contains("real-rom execution: exit="));

    let first_stderr = String::from_utf8_lossy(&first.stderr);
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    let first_trace = first_stderr
        .lines()
        .filter(|line| line.starts_with("[generated-trace]"))
        .collect::<Vec<_>>();
    let second_trace = second_stderr
        .lines()
        .filter(|line| line.starts_with("[generated-trace]"))
        .collect::<Vec<_>>();
    assert!(!first_trace.is_empty(), "generated execution trace must be observable");
    assert_eq!(
        first_trace, second_trace,
        "repeated real-ROM runs must produce an identical generated trace"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn real_rom_boot_checkpoint_is_deterministic() {
    let Some(path) = real_rom_path() else {
        eprintln!(
            "skipping real-ROM boot checkpoint validation: set {REAL_ROM_ENV} to a local .gba ROM"
        );
        return;
    };

    let (root, identity) = build_generated_real_rom(&path);
    let first = run_generated_runner(&root, &path, BOOT_STEP_LIMIT);
    let second = run_generated_runner(&root, &path, BOOT_STEP_LIMIT);

    assert!(
        first.status.success(),
        "first deterministic boot run failed for {}\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "second deterministic boot run failed for {}\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    let checkpoint = |stdout: &str| {
        stdout
            .lines()
            .filter(|line| line.starts_with("real-rom execution:"))
            .collect::<Vec<_>>()
    };
    let first_checkpoint = checkpoint(&first_stdout);
    let second_checkpoint = checkpoint(&second_stdout);

    assert!(!first_checkpoint.is_empty(), "boot checkpoint must be observable");
    assert!(
        first_checkpoint.iter().any(|line| line.contains("pc=")),
        "boot checkpoint must contain PC"
    );
    assert!(
        first_checkpoint.iter().any(|line| line.contains("thumb=")),
        "boot checkpoint must contain ARM/Thumb state"
    );
    assert!(
        first_checkpoint.iter().any(|line| line.contains("sp=")),
        "boot checkpoint must contain stack pointer"
    );
    assert!(
        first_checkpoint.iter().any(|line| line.contains("cycles=")),
        "boot checkpoint must contain machine cycles"
    );
    assert_ne!(
        first_checkpoint
            .iter()
            .find(|line| line.contains("pc="))
            .map(|line| *line)
            .expect("PC checkpoint is present"),
        "real-rom execution: pc=0x08000000"
    );
    assert_eq!(
        first_checkpoint, second_checkpoint,
        "repeated boot runs must produce an identical architectural checkpoint"
    );

    println!("real-rom boot checkpoint: {identity}");
    for line in &first_checkpoint {
        println!("{line}");
    }

    let first_stderr = String::from_utf8_lossy(&first.stderr);
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    let first_trace = first_stderr
        .lines()
        .filter(|line| line.starts_with("[generated-trace]"))
        .collect::<Vec<_>>();
    let second_trace = second_stderr
        .lines()
        .filter(|line| line.starts_with("[generated-trace]"))
        .collect::<Vec<_>>();
    assert!(!first_trace.is_empty(), "boot generated trace must be observable");
    assert_eq!(
        first_trace, second_trace,
        "repeated boot runs must produce an identical generated trace"
    );

    let _ = fs::remove_dir_all(&root);
}
