use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping, Mode};
use gba_runtime::{Cartridge, Runtime};

const REAL_ROM_ENV: &str = "GBA_REAL_ROM";
const DEFAULT_STEP_LIMIT: u64 = 512;
const CARTRIDGE_BASE: u32 = 0x0800_0000;

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

fn real_rom_path() -> Option<PathBuf> {
    env::var_os(REAL_ROM_ENV).map(PathBuf::from)
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

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root");
    let runtime_path = workspace_root
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
    let mut runtime = gba_runtime::Runtime::new();
    runtime.load_cartridge(gba_runtime::Cartridge::from_rom(rom, "saves"));

    let result = gba_generated::gba_entry_with_limit(&mut runtime, max_steps)?;
    assert_eq!(
        result.state.pc() % if result.state.thumb { 2 } else { 4 },
        0,
        "generated execution must preserve architectural PC alignment"
    );

    println!("real-rom execution: steps={}", result.steps);
    println!("real-rom execution: pc={:#010x}", result.state.pc());
    println!("real-rom execution: thumb={}", result.state.thumb);
    println!("real-rom execution: cycles={}", result.state.cycles);
    println!("real-rom execution: exit={:?}", result.exit);

    match result.exit {
        GeneratedExecutionExit::Returned { .. }
        | GeneratedExecutionExit::Halted { .. }
        | GeneratedExecutionExit::StepLimitExceeded { .. } => Ok(()),
        GeneratedExecutionExit::ExceptionVector { kind, .. } => {
            Err(format!("execution escaped into an unlinked exception vector: {kind:?}").into())
        }
    }
}
"#,
    )
    .expect("write generated real-ROM runner");
}

#[test]
fn real_rom_execution_validates_cartridge_cfg_and_runtime_boundary() {
    let Some(path) = real_rom_path() else {
        eprintln!(
            "skipping real-ROM execution validation: set {REAL_ROM_ENV} to a local .gba ROM"
        );
        return;
    };

    let rom = fs::read(&path).expect("GBA_REAL_ROM must point to a readable ROM");
    assert!(
        rom.len() >= 0xc0,
        "real GBA ROM must contain the minimum cartridge header"
    );

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

    let mut preflight = Runtime::new();
    preflight.load_cartridge(Cartridge::from_rom(rom.clone(), "saves"));
    assert_eq!(preflight.cpu.r[15], CARTRIDGE_BASE);
    assert_eq!(preflight.read8(CARTRIDGE_BASE), rom[0]);
    assert_eq!(
        preflight.read16(CARTRIDGE_BASE),
        u16::from_le_bytes([rom[0], rom[1]])
    );
    assert_eq!(
        preflight.read32(CARTRIDGE_BASE),
        u32::from_le_bytes([rom[0], rom[1], rom[2], rom[3]])
    );

    let root = unique_runner_dir();
    write_generated_runner(&root, &generated.source);

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .arg("--")
        .arg(&path)
        .arg(DEFAULT_STEP_LIMIT.to_string())
        .output()
        .expect("cargo must be available for real-ROM execution validation");

    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "real-ROM generated runner failed for {}\nstdout:\n{}\nstderr:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("real-rom execution: steps="));
    assert!(stdout.contains("real-rom execution: pc="));
    assert!(stdout.contains("real-rom execution: cycles="));
}
