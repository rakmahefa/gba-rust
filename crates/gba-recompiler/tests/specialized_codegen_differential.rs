use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gba_recompiler::{
    analyze, build_semantic_program, discover_functions, generate_semantic, Mode, ROM_BASE,
};
use gba_runtime::{CPSR_C, CPSR_N, CPSR_V, CPSR_Z};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}
fn thumb_rom(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn execute_generated(source: &str, setup: &str) -> [u64; 5] {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("gba-specialized-{id}"));
    fs::create_dir_all(root.join("src")).expect("temporary project");
    let generated = root.join("src/generated.rs");
    let main_rs = root.join("src/main.rs");
    let cargo_toml = root.join("Cargo.toml");
    let binary = root.join("target/debug/gba_generated_specialized_test");
    fs::write(&generated, source).expect("generated source");
    let runtime_path = workspace_root().join("crates/gba-runtime");
    fs::write(
        &cargo_toml,
        format!(
            "[package]\nname = \"gba_generated_specialized_test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ngba-runtime = {{ package = \"gba-runtime\", path = \"{}\" }}\n",
            runtime_path.display()
        ),
    )
    .expect("Cargo manifest");
    fs::write(
        &main_rs,
        format!(
            "mod generated {{ include!(r#\"{}\"#); }}\nfn main() {{ let mut rt = gba_runtime::Runtime::new(); {} let result = generated::entry(&mut rt).expect(\"generated execution\"); println!(\"{{}} {{}} {{}} {{}} {{}}\", result.state.registers[0], result.state.registers[1], result.state.registers[2], result.state.cpsr, result.steps); }}\n",
            generated.display(), setup
        ),
    )
    .expect("wrapper source");

    let compile = Command::new("cargo")
        .arg("build")
        .arg("--offline")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&cargo_toml)
        .status()
        .expect("cargo build");
    assert!(compile.success(), "generated Rust did not compile");
    assert!(
        binary.is_file(),
        "cargo build succeeded but generated binary is missing at {}",
        binary.display()
    );

    let output = Command::new(&binary).output().expect("generated binary");
    assert!(
        output.status.success(),
        "generated Rust failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values = String::from_utf8(output.stdout)
        .expect("UTF-8 output")
        .split_whitespace()
        .map(|value| value.parse::<u64>().expect("integer output"))
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 5, "unexpected generated output: {values:?}");
    let result = values.try_into().expect("five generated values");
    let _ = fs::remove_dir_all(&root);
    result
}

fn generate_arm(words: &[u32]) -> String {
    let program = analyze(&arm_rom(words), ROM_BASE, Mode::Arm).expect("ARM analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("ARM semantic lowering");
    generate_semantic(&program, &semantic, "entry").source
}

fn generate_thumb(words: &[u16]) -> String {
    let program = analyze(&thumb_rom(words), ROM_BASE, Mode::Thumb).expect("Thumb analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("Thumb semantic lowering");
    generate_semantic(&program, &semantic, "entry").source
}

#[test]
fn specialized_codegen_has_no_raw_instruction_dispatch() {
    let source = generate_arm(&[0xE3A0_0001, 0xE280_1002, 0xE241_2001]);
    assert!(!source.contains("execute_arm_instruction"));
    assert!(!source.contains("execute_thumb_instruction"));
    assert!(source.contains("rt.add(1"));
    assert!(source.contains("rt.sub(2"));
}

#[test]
fn specialized_arm_arithmetic_matches_reference() {
    let source = generate_arm(&[0xE3A0_0001, 0xE280_1002, 0xE241_2001, 0xE251_3002]);
    let actual = execute_generated(&source, "");
    assert_eq!([actual[0], actual[1], actual[2]], [1, 3, 2]);
    assert_eq!(
        actual[3] as u32 & (CPSR_N | CPSR_Z | CPSR_C | CPSR_V),
        CPSR_C
    );
    assert_eq!(actual[4], 1);
}

#[test]
fn specialized_arm_memory_and_multiply_match_reference() {
    let source = generate_arm(&[0xE3A0_102A, 0xE580_1000, 0xE590_2000, 0xE000_0291]);
    let actual = execute_generated(&source, "rt.write_reg(0, 0x0400_0004);");
    assert_eq!(actual[0], 42 * 42);
    assert_eq!(actual[1], 42);
    assert_eq!(actual[2], 42);
    assert_eq!(actual[4], 1);
}

#[test]
fn specialized_arm_logic_and_shift_codegen_is_executable() {
    let source = generate_arm(&[0xE3A0_0003, 0xE1A0_1080, 0xE201_2007, 0xE381_3001]);
    let actual = execute_generated(&source, "");
    assert_eq!(actual[0], 3);
    assert_eq!(actual[1], 6);
    assert_eq!(actual[2], 6);
    assert_eq!(actual[3] as u32 & (CPSR_N | CPSR_Z), 0);
}

#[test]
fn specialized_thumb_alu_matches_reference() {
    let source = generate_thumb(&[0x2003, 0x2107, 0x4008, 0x4048, 0x4308, 0x4388, 0x43C8]);
    let actual = execute_generated(&source, "");
    assert_eq!(actual[0] as u32, !7u32);
    assert_eq!(actual[1], 7);
    assert_ne!(actual[3] as u32 & CPSR_N, 0);
    assert_eq!(actual[4], 1);
}

#[test]
fn semantic_contract_rejects_control_tampering() {
    let program =
        analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).expect("analysis");
    let functions = discover_functions(&program);
    let mut semantic = build_semantic_program(&program, &functions).expect("semantic");
    semantic.functions[0].blocks[0].instructions[0]
        .ops
        .push(gba_recompiler::IrOp::Branch {
            target: ROM_BASE,
            condition: gba_recompiler::Condition::Al,
            link: false,
        });
    let error = gba_recompiler::validate_semantic_program(&program, &functions, &semantic)
        .expect_err("tampering must fail");
    assert!(error.contains("instruction control effect changed"));
}
