use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gba_recompiler::{
    analyze, build_semantic_program, discover_functions, generate_semantic, Mode, ROM_BASE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReferenceResult {
    r0: u32,
    r1: u32,
    blocks: u64,
    cycles: u64,
    pc: u32,
}

fn reference_loop() -> ReferenceResult {
    let mut r0 = 0u32;
    let mut blocks = 0u64;
    let mut cycles = 0u64;

    blocks += 1; // block 0: mov r0, #0
    cycles += 1;
    for _ in 0..3 {
        blocks += 1; // block 1: add/cmp/bne
        cycles += 3;
        r0 = r0.wrapping_add(1);
    }
    blocks += 1; // block 2: mov r1, #42
    cycles += 1;

    ReferenceResult {
        r0,
        r1: 42,
        blocks,
        cycles,
        pc: ROM_BASE + 20,
    }
}

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn compile_and_run_generated(source: &str, assertions: &str) {
    let root = std::env::temp_dir().join(format!(
        "gba-block-linking-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("src")).expect("temporary test directory");
    let generated_path = root.join("src/generated.rs");
    let wrapper_path = root.join("src/main.rs");
    let manifest_path = root.join("Cargo.toml");
    fs::write(&generated_path, source).expect("generated source");
    fs::write(
        &wrapper_path,
        format!(
            "mod generated {{ include!(r#\"{}\"#); }}\nfn main() {{ let mut rt = gba_runtime::Runtime::new(); let result = generated::entry_with_limit(&mut rt, 100).expect(\"generated execution\"); {} }}\n",
            generated_path.display(),
            assertions
        ),
    )
    .expect("wrapper source");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_path = workspace_root.join("../gba-runtime");
    let manifest = format!(
        "[package]\nname = \"gba-generated-block-linking-test\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
        runtime_path.display()
    );
    fs::write(&manifest_path, manifest).expect("temporary Cargo manifest");

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .expect("cargo invocation");

    assert!(
        output.status.success(),
        "generated dispatch/linking failed:\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        source
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn generated_arm_loop_matches_independent_reference_and_dispatches_backwards_edge() {
    let words = [
        0xE3A0_0000, // mov r0, #0
        0xE280_0001, // add r0, r0, #1
        0xE350_0003, // cmp r0, #3
        0x1AFF_FFFC, // bne 0x0800_0004
        0xE3A0_102A, // mov r1, #42
    ];
    let program = analyze(&arm_rom(&words), ROM_BASE, Mode::Arm).expect("analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("semantic lowering");
    let generated = generate_semantic(&program, &semantic, "entry");

    assert!(generated.source.contains("fn dispatch_block"));
    assert!(generated.source.contains("fn is_linked_block"));
    assert!(generated.source.contains("0x08000004"));
    assert!(generated.source.contains("0x08000010"));

    let reference = reference_loop();
    compile_and_run_generated(
        &generated.source,
        &format!(
            "assert_eq!(result.state.registers[0], {}); assert_eq!(result.state.registers[1], {}); assert_eq!(result.steps, {}); assert_eq!(result.state.cycles, {}); assert_eq!(result.state.pc(), {:#010x}); assert_eq!(result.exit, gba_runtime::GeneratedExecutionExit::Halted {{ address: {:#010x}, thumb: false }});",
            reference.r0,
            reference.r1,
            reference.blocks,
            reference.cycles,
            reference.pc,
            reference.pc,
        ),
    );
}
