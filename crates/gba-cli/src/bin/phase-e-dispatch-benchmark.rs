use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gba_runtime::{
    run_generated_linked, GeneratedBlockExit, GeneratedExecutionExit, GeneratedLinkedBlock,
    LinkedBlockExit, Runtime, RuntimeContract,
};

const ENTRY: u32 = 0x0800_0000;
const NEXT: u32 = 0x0800_0004;
const ITERATIONS: u64 = 100_000;
const SAMPLES: usize = 5;

fn run_contract_sample(iterations: u64) -> (Duration, u64, u64) {
    let mut runtime = Runtime::new();
    let linked_probes = AtomicU64::new(0);
    let start = Instant::now();
    let result = runtime
        .run_generated_contract(
            ENTRY,
            false,
            Some(iterations),
            |_, _, _| Ok(GeneratedBlockExit::continue_to(NEXT, false)),
            |address, thumb| {
                linked_probes.fetch_add(1, Ordering::Relaxed);
                address == NEXT && !thumb
            },
        )
        .expect("contract dispatch target must remain linked");

    let steps = match result.exit {
        GeneratedExecutionExit::StepLimitExceeded { .. } => result.steps,
        _ => panic!("contract benchmark must terminate through the step limit"),
    };
    (
        start.elapsed(),
        steps,
        linked_probes.load(Ordering::Relaxed),
    )
}

fn linked_first(_: &mut Runtime) -> Result<LinkedBlockExit, &'static str> {
    Ok(LinkedBlockExit::Next {
        block: linked_loop,
        address: NEXT,
        thumb: false,
    })
}

fn linked_loop(_: &mut Runtime) -> Result<LinkedBlockExit, &'static str> {
    Ok(LinkedBlockExit::Next {
        block: linked_loop,
        address: NEXT,
        thumb: false,
    })
}

fn run_linked_sample(iterations: u64) -> (Duration, u64) {
    let mut runtime = Runtime::new();
    let start = Instant::now();
    let result = run_generated_linked(
        &mut runtime,
        linked_first as GeneratedLinkedBlock,
        ENTRY,
        false,
        Some(iterations),
    )
    .expect("linked execution must terminate through the step limit");
    let steps = match result.exit {
        GeneratedExecutionExit::StepLimitExceeded { .. } => result.steps,
        _ => panic!("linked benchmark must terminate through the step limit"),
    };
    (start.elapsed(), steps)
}

fn main() {
    let mut contract_total = Duration::ZERO;
    let mut linked_total = Duration::ZERO;
    let mut contract_steps = 0u64;
    let mut linked_steps = 0u64;
    let mut contract_probes = 0u64;

    for _ in 0..SAMPLES {
        let (elapsed, steps, probes) = run_contract_sample(ITERATIONS);
        contract_total += elapsed;
        contract_steps += steps;
        contract_probes += probes;

        let (elapsed, steps) = run_linked_sample(ITERATIONS);
        linked_total += elapsed;
        linked_steps += steps;
    }

    let contract_ns_per_step = contract_total.as_nanos() as f64 / contract_steps as f64;
    let linked_ns_per_step = linked_total.as_nanos() as f64 / linked_steps as f64;
    let speedup = contract_ns_per_step / linked_ns_per_step;

    black_box((contract_steps, linked_steps, contract_probes));

    println!("generated-dispatch phase-e benchmark");
    println!("samples={SAMPLES} iterations/sample={ITERATIONS}");
    println!("contract: steps={contract_steps} cfg_membership_probes={contract_probes}");
    println!("contract: elapsed={contract_total:?} ns/step={contract_ns_per_step:.2}");
    println!("linked:   steps={linked_steps} cfg_membership_probes=0");
    println!("linked:   elapsed={linked_total:?} ns/step={linked_ns_per_step:.2}");
    println!("direct_link_speedup={speedup:.3}x");
}
