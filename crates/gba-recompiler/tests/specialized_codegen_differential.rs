use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gba_recompiler::{
    analyze, build_semantic_program, discover_functions, generate_semantic, Condition, IrOp, Mode,
    ROM_BASE,
};
use gba_runtime::{CPSR_C, CPSR_N, CPSR_V, CPSR_Z};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}
fn thumb_rom(halfwords: &[u16]) -> Vec<u8> {
    halfwords
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

fn execute_generated(source: &str, setup: &str) -> [u64; 5] {
    let root = std::env::temp_dir().join(format!(
        "gba-specialized-diff-{}",
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
            "mod generated {{ include!(r#\"{}\"#); }}\nfn main() {{ let mut rt = gba_runtime::Runtime::new(); {} let result = generated::entry(&mut rt).expect(\"generated execution\"); println!(\"{{}} {{}} {{}} {{}} {{}}\", result.state.registers[0], result.state.registers[1], result.state.registers[2], result.state.cpsr, result.steps); }}\n",
            generated_path.display(), setup
        ),
    )
    .expect("wrapper source");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime_path = workspace_root.join("../gba-runtime");
    let manifest = format!(
        "[package]\nname = \"gba-generated-specialized-diff\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\ngba-runtime = {{ path = \"{}\" }}\n",
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
        "generated Rust failed to compile or execute (status={}):\nstdout:\n{}\nstderr:\n{}\nmanifest:\n{}\nsource:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&fs::read(&manifest_path).expect("manifest bytes")),
        source
    );

    let line = String::from_utf8_lossy(&output.stdout);
    let values = line
        .split_whitespace()
        .map(|value| value.parse::<u64>().expect("generated numeric output"))
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 5, "generated output: {line}");

    let _ = fs::remove_dir_all(root);
    values.try_into().expect("five generated values")
}

fn generate_arm(words: &[u32]) -> String {
    let program = analyze(&arm_rom(words), ROM_BASE, Mode::Arm).expect("ARM fixture analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("ARM semantic lowering");
    generate_semantic(&program, &semantic, "entry").source
}

fn generate_thumb(halfwords: &[u16]) -> String {
    let program =
        analyze(&thumb_rom(halfwords), ROM_BASE, Mode::Thumb).expect("Thumb analysis");
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
    let actual = execute_generated(&source, "rt.write_reg(0, 0x0300_0000);");
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
fn semantic_ir_rejects_codegen_contract_tampering_before_generation() {
    let program =
        analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).expect("analysis");
    let functions = discover_functions(&program);
    let mut semantic = build_semantic_program(&program, &functions).expect("semantic");
    semantic.functions[0].blocks[0].instructions[0]
        .ops
        .push(IrOp::Branch {
            target: ROM_BASE,
            condition: Condition::Al,
            link: false,
        });
    let error = gba_recompiler::validate_semantic_program(&program, &functions, &semantic)
        .expect_err("tampered semantic contract must fail");
    assert!(error.contains("instruction control effect changed"));
}
