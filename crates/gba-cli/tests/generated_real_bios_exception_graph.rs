use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gba_recompiler::{
    analyze_exception_graph, analyze_with_mapping, generate, ExceptionVectorKind, ImageKind,
    ImageMapping, Mode,
};

const REAL_BIOS: &[u8] = include_bytes!("../../../bios/gba_bios.bin");

fn unique_runner_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "gba-real-bios-exception-{stamp}-{}",
        std::process::id()
    ))
}

#[test]
fn real_bios_builds_exception_graph_and_executes_irq_vector() {
    let mapping = ImageMapping::new(
        ImageKind::Bios,
        0,
        REAL_BIOS.len() as u32,
        0,
        Mode::Arm,
    );
    let graph = analyze_exception_graph(REAL_BIOS, mapping).expect("real BIOS exception graph must analyze");

    assert_eq!(graph.nodes.len(), ExceptionVectorKind::ALL.len());
    for kind in ExceptionVectorKind::ALL {
        let node = graph.node(kind).expect("every architectural vector must be present");
        assert_eq!(node.vector, kind.vector());
        assert!(!node.program.cfg.blocks.is_empty());
    }
    assert!(
        graph
            .node(ExceptionVectorKind::Irq)
            .expect("IRQ vector")
            .exception_return_sites
            .iter()
            .all(|address| graph.image.contains(*address))
    );

    let irq_mapping = ImageMapping::new(
        ImageKind::Bios,
        0,
        REAL_BIOS.len() as u32,
        ExceptionVectorKind::Irq.vector(),
        Mode::Arm,
    );
    let irq_program = analyze_with_mapping(REAL_BIOS, irq_mapping).expect("real BIOS IRQ vector must analyze");
    let generated = generate(&irq_program, "gba_irq_entry");
    assert!(generated.source.contains("GeneratedBlockExit"));
    assert!(generated.source.contains("return_from_exception"));

    let root = unique_runner_dir();
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create generated real-BIOS runner");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root");
    let runtime_path = workspace_root
        .join("crates/gba-runtime")
        .canonicalize()
        .expect("resolve gba-runtime path");
    let bios_path = root.join("gba_bios.bin");
    fs::write(&bios_path, REAL_BIOS).expect("write real BIOS fixture");
    fs::write(src.join("gba_generated.rs"), generated.source).expect("write generated IRQ module");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"gba-real-bios-exception-e2e\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )
    .expect("write temporary Cargo manifest");
    fs::write(
        src.join("main.rs"),
        r#"mod gba_generated;

use std::{env, fs};

use gba_runtime::{CpuMode, ExceptionKind, GeneratedExecutionExit, Runtime, REG_PC};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bios_path = env::args().nth(1).ok_or("missing BIOS path")?;
    let bios = fs::read(bios_path)?;
    let mut runtime = Runtime::new();
    runtime.load_bios(&bios)?;

    let caller_cpsr = runtime.cpu.cpsr;
    let (vector, thumb) = runtime.raise_exception_at_boundary(ExceptionKind::Irq, 0x0000_0100, false);
    assert_eq!((vector, thumb), (0x18, false));
    assert_eq!(runtime.mode(), CpuMode::Irq);
    assert_eq!(runtime.cpu.r[REG_PC], 0x18);

    let result = gba_generated::gba_irq_entry_with_limit(&mut runtime, 1)?;

    assert_eq!(result.steps, 1);
    assert!(matches!(
        result.exit,
        GeneratedExecutionExit::StepLimitExceeded { .. }
            | GeneratedExecutionExit::Returned { .. }
            | GeneratedExecutionExit::Halted { .. }
    ));

    if matches!(result.exit, GeneratedExecutionExit::Returned { .. }) {
        assert_eq!(runtime.mode(), CpuMode::System);
        assert_eq!(runtime.cpu.cpsr, caller_cpsr);
        assert!(!runtime.cpu.thumb);
    } else {
        assert_eq!(runtime.mode(), CpuMode::Irq);
    }

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
        .expect("cargo must be available for real BIOS execution");

    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "real BIOS generated runner failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
