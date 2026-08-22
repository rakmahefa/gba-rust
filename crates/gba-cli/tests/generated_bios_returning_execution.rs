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
    std::env::temp_dir().join(format!(
        "gba-bios-generated-return-e2e-{stamp}-{}",
        std::process::id()
    ))
}

#[test]
fn generated_bios_returning_swi_resumes_cfg_and_mutates_runtime_memory() {
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
    let instructions = [
        0xE3A0_0001u32,
        0xEF00_0001u32,
        0xE3A0_1042u32,
        0xEF00_0002u32,
    ];
    for (index, instruction) in instructions.into_iter().enumerate() {
        let offset = index * 4;
        bios[offset..offset + 4].copy_from_slice(&instruction.to_le_bytes());
    }
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

    fs::write(src.join("gba_generated.rs"), generated.source)
        .expect("write generated Rust");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gba-bios-generated-return-e2e\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )
    .expect("write temporary Cargo manifest");
    fs::write(
        src.join("main.rs"),
        r#"#![allow(dead_code)]

mod gba_generated;

use std::{env, fs};

use gba_runtime::{GeneratedExecutionExit, PowerState, Runtime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bios_path = env::args().nth(1).ok_or("missing BIOS path")?;
    let bios = fs::read(bios_path)?;
    let mut runtime = Runtime::new();
    runtime.ewram.fill(0xA5);
    runtime.load_bios(&bios)?;

    let result = gba_generated::gba_entry_with_limit(&mut runtime, 8)?;

    println!("exit={:?}", result.exit);
    println!("steps={}", result.steps);
    println!("pc={:#010x}", result.state.pc());
    println!("r0={:#010x}", result.state.registers[0]);
    println!("r1={:#010x}", result.state.registers[1]);
    println!("power={:?}", runtime.power);
    println!("ewram0={:#04x}", runtime.ewram[0]);

    assert!(matches!(
        result.exit,
        GeneratedExecutionExit::Halted { address: 16, thumb: false }
    ));
    // Generated steps count dispatched basic blocks, not individual instructions.
    // This fixture has one block for SWI #1 and one for the post-SWI continuation.
    assert_eq!(result.steps, 2);
    assert_eq!(result.state.pc(), 16);
    assert_eq!(result.state.registers[0], 1);
    assert_eq!(result.state.registers[1], 0x42);
    assert_eq!(runtime.power, PowerState::Halted);
    assert!(runtime.ewram.iter().all(|byte| *byte == 0));

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
        "generated returning BIOS runner failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("exit=Halted"));
    assert!(stdout.contains("steps=2"));
    assert!(stdout.contains("pc=0x00000010"));
    assert!(stdout.contains("r1=0x00000042"));
    assert!(stdout.contains("power=Halted"));
    assert!(stdout.contains("ewram0=0x00"));
}
