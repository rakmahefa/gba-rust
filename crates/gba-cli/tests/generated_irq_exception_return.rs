use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

use gba_recompiler::{analyze_with_mapping, generate, ImageKind, ImageMapping, Mode};

fn unique_runner_dir() -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock must be after unix epoch").as_nanos();
    std::env::temp_dir().join(format!("gba-generated-irq-return-{stamp}-{}", std::process::id()))
}

#[test]
fn generated_irq_handler_restores_cpsr_and_resumes_caller_target() {
    let root = unique_runner_dir();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create generated runner");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("resolve workspace root");
    let runtime_path = workspace_root.join("crates/gba-runtime").canonicalize().expect("resolve runtime crate");

    // IRQ vector 0x18: SUBS PC, LR, #4. This is the architectural ARM IRQ return form.
    let mut bios = vec![0u8; 0x1c];
    bios[0x18..0x1c].copy_from_slice(&0xE25E_F004u32.to_le_bytes());
    let bios_path = root.join("bios.bin");
    fs::write(&bios_path, &bios).expect("write BIOS fixture");

    let mapping = ImageMapping::new(ImageKind::Bios, 0, bios.len() as u32, 0x18, Mode::Arm);
    let program = analyze_with_mapping(&bios, mapping).expect("BIOS IRQ fixture must analyze");
    let generated = generate(&program, "gba_irq_entry");
    assert!(generated.source.contains("return_from_exception"));

    fs::write(src.join("gba_generated.rs"), generated.source).expect("write generated Rust");
    fs::write(root.join("Cargo.toml"), format!("[package]\nname = \"gba-generated-irq-return\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n", runtime_path.display())).expect("write manifest");
    fs::write(src.join("main.rs"), r#"mod gba_generated;

use gba_runtime::{CpuMode, ExceptionKind, GeneratedExecutionExit, Runtime, REG_PC};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::new();
    runtime.enter_instruction(0x0800_0100, false);
    let caller_cpsr = runtime.cpu.cpsr;

    runtime.raise_exception(ExceptionKind::Irq);
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.read_reg(14), 0x0800_0104);

    let result = gba_generated::gba_irq_entry_with_limit(&mut runtime, 2)?;

    assert!(matches!(result.exit, GeneratedExecutionExit::Returned { address: 0x0800_0100, thumb: false }));
    assert_eq!(result.state.pc(), 0x0800_0100);
    assert_eq!(runtime.read_reg(REG_PC), 0x0800_0100);
    assert_eq!(runtime.mode(), CpuMode::System);
    assert_eq!(runtime.cpu.cpsr, caller_cpsr);
    assert!(!runtime.cpu.thumb);

    Ok(())
}
"#).expect("write runner main");

    let output = Command::new("cargo").arg("run").arg("--quiet").arg("--manifest-path").arg(root.join("Cargo.toml")).arg("--").arg(&bios_path).output().expect("cargo must be available");
    let _ = fs::remove_dir_all(&root);

    assert!(output.status.success(), "generated IRQ return runner failed\nstdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}
