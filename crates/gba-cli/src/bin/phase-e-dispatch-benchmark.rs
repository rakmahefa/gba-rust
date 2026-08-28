use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gba_runtime::{GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract};

const ENTRY: u32 = 0x0800_0000;
const NEXT: u32 = 0x0800_0004;
const ITERATIONS: u64 = 100_000;
const SAMPLES: usize = 5;

fn run_sample(iterations: u64) -> (Duration, u64, u64) {
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
        .expect("benchmark dispatch target must remain linked");

    let steps = match result.exit {
        GeneratedExecutionExit::StepLimitExceeded { .. } => result.steps,
        _ => panic!("benchmark must terminate through the step limit"),
    };
    (
        start.elapsed(),
        steps,
        linked_probes.load(Ordering::Relaxed),
    )
}

fn main() {
    let mut total = Duration::ZERO;
    let mut total_steps = 0u64;
    let mut total_probes = 0u64;

    for _ in 0..SAMPLES {
        let (elapsed, steps, probes) = run_sample(ITERATIONS);
        total = total.saturating_add(elapsed);
        total_steps = total_steps.saturating_add(steps);
        total_probes = total_probes.saturating_add(probes);
    }

    let ns_per_step = total.as_nanos() as f64 / total_steps as f64;
    println!("generated-dispatch baseline");
    println!("samples={SAMPLES} iterations/sample={ITERATIONS}");
    println!("steps={total_steps} linked_probes={total_probes}");
    println!("elapsed={total:?} ns/step={ns_per_step:.2}");
}
