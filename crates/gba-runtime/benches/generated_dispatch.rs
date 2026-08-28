use std::time::{Duration, Instant};

use gba_runtime::{GeneratedBlockExit, GeneratedExecutionExit, Runtime, RuntimeContract};

const ENTRY: u32 = 0x0800_0000;
const NEXT: u32 = 0x0800_0004;
const ITERATIONS: u64 = 100_000;
const SAMPLES: usize = 5;

#[derive(Debug)]
struct Sample {
    elapsed: Duration,
    steps: u64,
    linked_probes: u64,
}

fn run_sample(iterations: u64) -> Sample {
    let mut runtime = Runtime::new();
    let mut linked_probes = 0u64;
    let start = Instant::now();
    let result = runtime
        .run_generated_contract(
            ENTRY,
            false,
            Some(iterations),
            |_, _, _| Ok(GeneratedBlockExit::continue_to(NEXT, false)),
            |address, thumb| {
                linked_probes = linked_probes.saturating_add(1);
                address == NEXT && !thumb
            },
        )
        .expect("benchmark dispatch target must remain linked");

    let elapsed = start.elapsed();
    let steps = match result.exit {
        GeneratedExecutionExit::StepLimitExceeded { .. } => result.steps,
        _ => panic!("benchmark must terminate through the step limit"),
    };
    Sample {
        elapsed,
        steps,
        linked_probes,
    }
}

fn main() {
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        samples.push(run_sample(ITERATIONS));
    }

    let total = samples
        .iter()
        .map(|sample| sample.elapsed)
        .fold(Duration::ZERO, |acc, elapsed| acc.saturating_add(elapsed));
    let total_steps: u64 = samples.iter().map(|sample| sample.steps).sum();
    let total_probes: u64 = samples.iter().map(|sample| sample.linked_probes).sum();

    let ns_per_step = total.as_nanos() as f64 / total_steps as f64;
    println!("generated-dispatch baseline");
    println!("samples={SAMPLES} iterations/sample={ITERATIONS}");
    println!("steps={total_steps} linked_probes={total_probes}");
    println!("elapsed={total:?} ns/step={ns_per_step:.2}");
}
