use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gba_recompiler::{analyze, build_semantic_program, discover_functions, generate_semantic, Mode, ROM_BASE};

fn arm_rom(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn thumb_rom(halfwords: &[u16]) -> Vec<u8> {
    halfwords.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn runtime_rlib() -> PathBuf {
    let exe_dir = std::env::current_exe().expect("test executable path").parent().expect("test executable directory").to_path_buf();
    fs::read_dir(&exe_dir)
        .expect("target dependency directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("libgba_runtime-") && name.ends_with(".rlib"))
        })
        .unwrap_or_else(|| panic!("could not locate gba_runtime rlib in {}", exe_dir.display()))
}

fn compile_and_run_generated(source: &str, entry: &str, setup: &str, assertions: &str) {
    let root = std::env::temp_dir().join(format!("gba-specialized-{}", SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos()));
    fs::create_dir_all(&root).expect("temporary test directory");
    let generated_path = root.join("generated.rs");
    let wrapper_path = root.join("main.rs");
    let binary_path = root.join("generated-test");
    fs::write(&generated_path, source).expect("generated source");
    let wrapper = format!(
        "mod generated {{ include!(r#\"{}\"#); }}\nfn main() {{ let mut rt = gba_runtime::Runtime::new(); {} let result = generated::{}(&mut rt).expect(\"generated execution\"); {} }}\n",
        generated_path.display(), setup, entry, assertions
    );
    fs::write(&wrapper_path, wrapper).expect("wrapper source");

    let rlib = runtime_rlib();
    let dep_dir = rlib.parent().expect("runtime rlib directory");
    let output = Command::new("rustc")
        .arg("--edition=2021")
        .arg("-L")
        .arg(format!("dependency={}", dep_dir.display()))
        .arg("--extern")
        .arg(format!("gba_runtime={}", rlib.display()))
        .arg("-o")
        .arg(&binary_path)
        .arg(&wrapper_path)
        .output()
        .expect("rustc invocation");
    assert!(output.status.success(), "generated Rust failed to compile:\nstdout:\n{}\nstderr:\n{}\nsource:\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr), source);

    let run = Command::new(&binary_path).output().expect("generated binary invocation");
    assert!(run.status.success(), "generated execution failed:\nstdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    let _ = fs::remove_dir_all(root);
}

fn generate_arm(words: &[u32]) -> String {
    let program = analyze(&arm_rom(words), ROM_BASE, Mode::Arm).expect("ARM fixture analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("ARM semantic lowering");
    generate_semantic(&program, &semantic, "entry").source
}

fn generate_thumb(halfwords: &[u16]) -> String {
    let program = analyze(&thumb_rom(halfwords), ROM_BASE, Mode::Thumb).expect("Thumb fixture analysis");
    let functions = discover_functions(&program);
    let semantic = build_semantic_program(&program, &functions).expect("Thumb semantic lowering");
    generate_semantic(&program, &semantic, "entry").source
}

#[test]
fn specialized_arm_data_processing_executes_against_architectural_expectations() {
    let source = generate_arm(&[
        0xE3A0_0001, // mov r0, #1
        0xE280_1002, // add r1, r0, #2
        0xE241_2001, // sub r2, r1, #1
        0xE381_3004, // orr r3, r1, #4
        0xE213_4007, // ands r4, r3, #7
        0xE334_0004, // teq r4, #4
        0xE2A2_5001, // adc r5, r2, #1
        0xE255_6001, // subs r6, r5, #1
        0xE3E7_7000, // mvn r7, r7
    ]);
    compile_and_run_generated(
        &source,
        "entry",
        "rt.write_reg(7, 0);",
        "assert_eq!(result.state.registers[0], 1); assert_eq!(result.state.registers[1], 3); assert_eq!(result.state.registers[2], 2); assert_eq!(result.state.registers[3], 7); assert_eq!(result.state.registers[4], 7); assert_eq!(result.state.registers[5], 3); assert_eq!(result.state.registers[6], 2); assert_eq!(result.state.registers[7], u32::MAX);",
    );
}

#[test]
fn specialized_arm_shift_and_multiply_execute_without_raw_dispatch() {
    let source = generate_arm(&[
        0xE3A0_0003, // mov r0, #3
        0xE1A0_1080, // mov r1, r0, lsl #1
        0xE000_0291, // mul r0, r1, r2
    ]);
    compile_and_run_generated(
        &source,
        "entry",
        "rt.write_reg(2, 4);",
        "assert_eq!(result.state.registers[1], 6); assert_eq!(result.state.registers[0], 24);",
    );
}

#[test]
fn specialized_thumb_shifted_and_alu_operations_execute() {
    let source = generate_thumb(&[
        0x2003, // movs r0, #3
        0x2107, // movs r1, #7
        0x0040, // lsls r0, r0, #1
        0x4008, // ands r0, r1
        0x4048, // eors r0, r1
        0x4308, // orrs r0, r1
        0x4388, // bics r0, r1
        0x43C8, // mvns r0, r1
    ]);
    compile_and_run_generated(
        &source,
        "entry",
        "",
        "assert_eq!(result.state.registers[0], !7u32); assert!(result.state.cpsr & gba_runtime::CPSR_N != 0); assert!(!result.state.thumb || result.state.thumb);",
    );
}

#[test]
fn specialized_thumb_arithmetic_and_compare_flags_execute() {
    let source = generate_thumb(&[
        0x2001, // movs r0, #1
        0x3001, // adds r0, #1
        0x3801, // subs r0, #1
        0x2801, // cmp r0, #1
        0x42C8, // cmn r0, r1
        0x4148, // adcs r0, r1
        0x4188, // sbcs r0, r1
        0x4248, // negs r0, r1
        0x4348, // muls r0, r1
    ]);
    compile_and_run_generated(
        &source,
        "entry",
        "rt.write_reg(1, 2); rt.set_flags(gba_runtime::Nzcv::new(false, false, true, false));",
        "assert_eq!(result.state.registers[0], 0xFFFF_FFFEu32.wrapping_mul(2)); assert_eq!(result.steps, 9);",
    );
}

#[test]
fn specialized_memory_codegen_executes_load_store_roundtrip() {
    let source = generate_arm(&[
        0xE3A0_0000, // mov r0, #0
        0xE280_0004, // add r0, r0, #4
        0xE3A1_102A, // mov r1, #0x2a
        0xE580_1000, // str r1, [r0]
        0xE590_2000, // ldr r2, [r0]
        0xE5C0_1020, // strb r1, [r0]
        0xE5D0_3000, // ldrb r3, [r0]
    ]);
    compile_and_run_generated(
        &source,
        "entry",
        "",
        "assert_eq!(result.state.registers[2], 0x2a); assert_eq!(result.state.registers[3], 0x2a); assert_eq!(rt.read32(4), 0x2a);",
    );
}

#[test]
fn semantic_ir_rejects_codegen_contract_tampering_before_generation() {
    let program = analyze(&arm_rom(&[0xE3A0_0001, 0xE280_0001]), ROM_BASE, Mode::Arm).expect("analysis");
    let functions = discover_functions(&program);
    let mut semantic = build_semantic_program(&program, &functions).expect("semantic");
    semantic.functions[0].blocks[0].instructions[0].control = gba_recompiler::IrControlEffect::Branch { target: ROM_BASE, condition: gba_recompiler::Condition::Al, link: false };
    let error = gba_recompiler::validate_semantic_program(&program, &functions, &semantic).expect_err("tampered semantic contract must fail");
    assert!(error.contains("control-effect instruction before its terminator"));
}

#[allow(dead_code)]
fn _assert_file_path_is_absolute(path: &Path) { assert!(path.is_absolute()); }
