use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::model::*;

/// Configuration for the random trace generator.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    pub instruction_count: usize,
    /// Pipeline stage names.
    pub stages: Vec<String>,
    /// Base duration for each stage (index-matched with `stages`).
    /// The actual duration is `base ± 1` with some probability.
    pub stage_durations: Vec<u32>,
    /// How many instructions the frontend can fetch per cycle.
    pub fetch_width: u32,
    /// Probability that a stage stalls for 1-2 extra cycles.
    pub stall_probability: f64,
    /// Probability that a branch triggers a pipeline flush.
    pub flush_probability: f64,
    /// Probability that an instruction depends on a recent producer.
    pub dependency_probability: f64,
    /// Number of synthetic performance counters to generate.
    pub counter_count: usize,
    pub seed: u64,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            instruction_count: 1000,
            stages: vec![
                "Fetch".into(),
                "Decode".into(),
                "Rename".into(),
                "Dispatch".into(),
                "Issue".into(),
                "Execute".into(),
                "Complete".into(),
                "Retire".into(),
            ],
            //                   Fe De Re Di Is Ex Co Re
            stage_durations: vec![1, 1, 1, 1, 1, 3, 1, 1],
            fetch_width: 4,
            stall_probability: 0.10,
            flush_probability: 0.01,
            dependency_probability: 0.3,
            counter_count: 0,
            seed: 42,
        }
    }
}

/// Generate a realistic superscalar pipeline trace.
///
/// Invariant: instruction i always starts its first stage at or before
/// instruction i+1.  Stages within one instruction are contiguous (no gaps).
pub fn generate(config: &GeneratorConfig) -> PipelineTrace {
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut trace = PipelineTrace::new();

    let stage_indices: Vec<StageNameIdx> = config
        .stages
        .iter()
        .map(|s| trace.intern_stage(s))
        .collect();
    let n_stages = stage_indices.len();

    let disasm_ops = [
        "add", "sub", "mul", "ldr", "str", "beq", "bne", "and", "orr", "eor", "lsl", "asr", "cmp",
        "mov", "nop", "ret",
    ];
    let regs = [
        "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13",
        "x14", "x15",
    ];

    // Pad or truncate stage_durations to match stages length.
    let base_durations: Vec<u32> = (0..n_stages)
        .map(|i| config.stage_durations.get(i).copied().unwrap_or(1).max(1))
        .collect();

    let width = config.fetch_width as usize;

    // Track the most recent first_cycle to enforce ordering.
    let mut prev_first_cycle: u32 = 0;

    // The cycle at which the next fetch group begins.
    let mut fetch_cycle: u32 = 0;
    let mut next_flush_end: Option<usize> = None;

    let mut i = 0usize;
    while i < config.instruction_count {
        let group_size = (config.instruction_count - i).min(width);

        for g in 0..group_size {
            // Generate disassembly.
            let op = disasm_ops[rng.gen_range(0..disasm_ops.len())];
            let rd = regs[rng.gen_range(0..regs.len())];
            let rs1 = regs[rng.gen_range(0..regs.len())];
            let rs2 = regs[rng.gen_range(0..regs.len())];
            let disasm = format!("{op} {rd}, {rs1}, {rs2}");

            // Flush handling.
            let is_flushed = if let Some(end) = next_flush_end {
                if i + g >= end {
                    next_flush_end = None;
                    false
                } else {
                    true
                }
            } else if op == "beq" || op == "bne" {
                if rng.gen_bool(config.flush_probability.min(1.0)) {
                    let flush_count = rng.gen_range(1..=4.min(config.instruction_count - (i + g)));
                    next_flush_end = Some(i + g + flush_count);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            let flush_after_stage = if is_flushed {
                Some(rng.gen_range(1..n_stages.max(2)))
            } else {
                None
            };

            let stage_start_idx = trace.stages.len() as u32;

            // The instruction enters at fetch_cycle, but must not start before
            // the previous instruction to preserve program order.
            let mut stage_start = fetch_cycle.max(prev_first_cycle);
            let first_cycle = stage_start;
            let mut last_cycle = stage_start;

            for (si, &stage_idx) in stage_indices.iter().enumerate() {
                // Duration: base ± variation + optional stall.
                let base = base_durations[si];
                let variation = if base > 1 && rng.gen_bool(0.3) {
                    if rng.gen_bool(0.5) {
                        1i32
                    } else {
                        -1
                    }
                } else {
                    0
                };
                let mut duration = (base as i32 + variation).max(1) as u32;

                // Occasional stall adds 1-2 cycles.
                if rng.gen_bool(config.stall_probability.min(1.0)) {
                    duration += rng.gen_range(1..=2);
                }

                let stage_end = stage_start + duration;

                // Cut short on flush.
                if let Some(fs) = flush_after_stage {
                    if si >= fs {
                        break;
                    }
                }

                trace.stages.push(StageSpan {
                    stage_name_idx: stage_idx,
                    lane: 0,
                    _pad: 0,
                    start_cycle: stage_start,
                    end_cycle: stage_end,
                });

                last_cycle = last_cycle.max(stage_end);

                // Contiguous: next stage starts immediately.
                stage_start = stage_end;
            }

            let stage_end_idx = trace.stages.len() as u32;
            prev_first_cycle = first_cycle;

            let retire_status = if is_flushed {
                RetireStatus::Flushed
            } else {
                RetireStatus::Retired
            };

            trace.add_instruction(InstructionData {
                id: (i + g) as u32,
                sim_id: (i + g) as u64,
                thread_id: 0,
                rbid: None,
                iq_id: None,
                dq_id: None,
                ready_cycle: None,
                disasm,
                tooltip: String::new(),
                stage_range: stage_start_idx..stage_end_idx,
                retire_status,
                first_cycle,
                last_cycle,
            });

            // Dependencies.
            if i + g > 0 && rng.gen_bool(config.dependency_probability.min(1.0)) {
                let producer = rng.gen_range((i + g).saturating_sub(8)..(i + g)) as u32;
                trace.dependencies.push(Dependency {
                    producer,
                    consumer: (i + g) as u32,
                    kind: match rng.gen_range(0..3) {
                        0 => DepKind::Data,
                        1 => DepKind::Control,
                        _ => DepKind::Memory,
                    },
                });
            }
        }

        i += group_size;
        fetch_cycle += 1;
    }

    // Generate synthetic performance counters.
    if config.counter_count > 0 {
        let max_cycle = trace.max_cycle() as usize;
        let counter_names = [
            "committed_insns",
            "cycles",
            "retired_insns",
            "mispredicts",
            "dcache_misses",
            "icache_misses",
            "dtlb_misses",
            "itlb_misses",
            "rob_full_stalls",
            "iq_full_stalls",
            "dq_full_stalls",
            "lsq_full_stalls",
            "fetch_bubbles",
            "decode_stalls",
            "br_taken",
            "br_not_taken",
            "load_ops",
            "store_ops",
            "alu_ops",
            "fp_ops",
        ];
        for ci in 0..config.counter_count {
            let name = if ci < counter_names.len() {
                counter_names[ci].to_string()
            } else {
                format!("counter_{}", ci)
            };
            // Generate cumulative counter values.
            // Each counter has a different average rate and burstiness.
            let avg_rate = rng.gen_range(0.1_f64..5.0);
            let burst_prob = rng.gen_range(0.01_f64..0.2);
            let mut cumulative = 0u64;
            let mut values = Vec::with_capacity(max_cycle + 1);
            for _ in 0..=max_cycle {
                if rng.gen_bool(burst_prob.min(1.0)) {
                    cumulative += rng.gen_range(1..=(avg_rate * 10.0) as u64 + 1);
                } else if rng.gen_bool((avg_rate / 2.0).min(1.0)) {
                    cumulative += rng.gen_range(1..=(avg_rate * 2.0) as u64 + 1);
                }
                values.push(cumulative);
            }
            trace.counters.push(CounterSeries {
                name,
                values,
                default_mode: if ci == 0 {
                    CounterDisplayMode::Rate
                } else {
                    CounterDisplayMode::Total
                },
            });
        }
    }

    trace
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_seed() {
        let config = GeneratorConfig {
            instruction_count: 100,
            seed: 123,
            ..Default::default()
        };
        let t1 = generate(&config);
        let t2 = generate(&config);
        assert_eq!(t1.instructions.len(), t2.instructions.len());
        for (a, b) in t1.instructions.iter().zip(t2.instructions.iter()) {
            assert_eq!(a.disasm, b.disasm);
            assert_eq!(a.first_cycle, b.first_cycle);
            assert_eq!(a.last_cycle, b.last_cycle);
        }
    }

    #[test]
    fn test_contiguous_stages() {
        let config = GeneratorConfig {
            instruction_count: 500,
            ..Default::default()
        };
        let trace = generate(&config);
        for row in 0..trace.row_count() {
            let spans = trace.stages_for(row);
            for w in spans.windows(2) {
                assert_eq!(
                    w[0].end_cycle,
                    w[1].start_cycle,
                    "Gap in row {}: {} ends at {} but next starts at {}",
                    row,
                    trace.stage_name(w[0].stage_name_idx),
                    w[0].end_cycle,
                    w[1].start_cycle,
                );
            }
        }
    }

    #[test]
    fn test_monotonic_first_cycle() {
        // Instruction i must start at or before instruction i+1.
        let config = GeneratorConfig {
            instruction_count: 500,
            ..Default::default()
        };
        let trace = generate(&config);
        for w in trace.instructions.windows(2) {
            assert!(
                w[0].first_cycle <= w[1].first_cycle,
                "Instruction {} starts at cycle {} but next instruction {} starts at cycle {}",
                w[0].id,
                w[0].first_cycle,
                w[1].id,
                w[1].first_cycle,
            );
        }
    }

    #[test]
    fn test_varied_durations() {
        // With default config, Execute stage has base duration 3.
        // Check that we get stages wider than 1 cycle.
        let config = GeneratorConfig {
            instruction_count: 100,
            stall_probability: 0.0, // no stalls, just base durations
            ..Default::default()
        };
        let trace = generate(&config);
        let mut found_wide = false;
        for row in 0..trace.row_count() {
            for span in trace.stages_for(row) {
                if span.end_cycle - span.start_cycle > 1 {
                    found_wide = true;
                    break;
                }
            }
            if found_wide {
                break;
            }
        }
        assert!(found_wide, "Expected some stages wider than 1 cycle");
    }

    #[test]
    fn test_large_scale() {
        let config = GeneratorConfig {
            instruction_count: 100_000,
            ..Default::default()
        };
        let trace = generate(&config);
        assert_eq!(trace.row_count(), 100_000);
        assert!(trace.max_cycle() > 0);
    }
}
