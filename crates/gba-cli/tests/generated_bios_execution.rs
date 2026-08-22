use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping, Mode};

fn unique_runner_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("gba-bios-generated-e2e-{stamp}-{}", std::process::id()))
}

#[test]
fn generated_bios_swi_executes_through_real_runtime_contract() {
    let root = unique_runner_dir();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create temporary generated runner");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root");
    let runtime_path = workspace_root
        .join("crates/gba-runtime")
        .canonicalize()
        .expect("resolve gba-runtime path");

    let mut bios = vec![0u8; 0x4000];
    bios[..4].copy_from_slice(&0xEF00_0002u32.to_le_bytes());
    let bios_path = root.join("bios.bin");
    fs::write(&bios_path, &bios).expect("write BIOS fixture");

    let mapping = ImageMapping::new(
        ImageKind::Bios,
        0x0000_0000,
        bios.len() as u32,
        0x0000_0000,
        Mode::Arm,
    );
    let program = analyze_with_mapping(&bios, mapping).expect("BIOS fixture must analyze");
    let generated = generate(&program, "gba_entry");
    assert!(generated.source.contains("execute_bios_swi_comment"));

    fs::write(src.join("gba_generated.rs"), generated.source).expect("write generated Rust");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gba-bios-generated-e2e\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )
    .expect("write temporary Cargo manifest");
    fs::write(
        src.join("main.rs"),
        r#"mod gba_generated;

use std::{env, fs};

use gba_runtime::{GeneratedExecutionExit, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bios_path = env::args().nth(1).ok_or("missing BIOS path")?;
    let bios = fs::read(bios_path)?;
    let mut runtime = Runtime::new();
    runtime.load_bios(&bios)?;

    let result = gba_generated::gba_entry_with_limit(&mut runtime, 4)?;

    println!("exit={:?}", result.exit);
    println!("steps={}", result.steps);
    println!("pc={:#010x}", result.state.pc());
    println!("thumb={}", result.state.thumb);
    println!("cycles={}", result.state.cycles);
    println!("power={:?}", runtime.power);

    assert!(matches!(result.exit, GeneratedExecutionExit::Halted { address: 4, thumb: false }));
    assert_eq!(result.steps, 1);
    assert_eq!(result.state.pc(), 4);
    assert!(!result.state.thumb);
    assert_eq!(runtime.power, gba_runtime::PowerState::Halted);

    Ok(())
}
"#,
    )
    .expect("write generated runner main");

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .arg("--")
        .arg(&bios_path)
        .output()
        .expect("cargo must be available for generated E2E execution");

    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "generated BIOS runner failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("exit=Halted"));
    assert!(stdout.contains("steps=1"));
    assert!(stdout.contains("pc=0x00000004"));
    assert!(stdout.contains("power=Halted"));
}
