use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

use super::model::*;
use super::{TraceError, TraceSource};

pub struct KonataSource;

impl TraceSource for KonataSource {
    fn format_name(&self) -> &str {
        "Konata"
    }

    fn file_extensions(&self) -> &[&str] {
        &["kanata", "konata", "log"]
    }

    fn load(&self, reader: &mut dyn Read) -> Result<PipelineTrace, TraceError> {
        parse_konata(reader)
    }

    fn detect(&self, first_bytes: &[u8]) -> bool {
        // Konata files typically start with "Kanata" header or "I" commands.
        let s = String::from_utf8_lossy(first_bytes);
        s.starts_with("Kanata") || s.lines().any(|l| l.starts_with("I\t"))
    }
}

/// State for an instruction being built during parsing.
struct InstrBuilder {
    id: u32,
    sim_id: u64,
    thread_id: u16,
    disasm: String,
    tooltip: String,
    stages: Vec<StageSpan>,
    retire_status: RetireStatus,
    first_cycle: u32,
    last_cycle: u32,
    /// Currently open stages per lane (not yet ended).
    open_stages: HashMap<u8, (StageNameIdx, u32)>, // lane -> (name_idx, start_cycle)
}

/// Parse a Konata/Kanata format trace file.
///
/// Format reference (simplified):
///   Kanata <version>     — header
///   C=<n>                — set cycle counter
///   C                    — increment cycle counter
///   I\t<id>\t<sim_id>\t<thread_id>  — new instruction
///   L\t<id>\t<lane>\t<text>         — label (disassembly)
///   S\t<id>\t<lane>\t<stage_name>   — stage start
///   E\t<id>\t<lane>\t<stage_name>   — stage end
///   R\t<id>\t<sim_id>\t<retire_type> — retire (0=retire, 1=flush)
///   W\t<id>\t<consumer_id>          — dependency
fn parse_konata(reader: &mut dyn Read) -> Result<PipelineTrace, TraceError> {
    let mut trace = PipelineTrace::new();
    let buf = BufReader::new(reader);
    let mut builders: HashMap<u32, InstrBuilder> = HashMap::new();
    let mut current_cycle: u32 = 0;
    let mut order: Vec<u32> = Vec::new(); // track insertion order

    for (line_num, line_result) in buf.lines().enumerate() {
        let line = line_result.map_err(TraceError::Io)?;
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with("Kanata") {
            continue;
        }

        if let Some(rest) = line.strip_prefix("C=") {
            let raw = rest.trim();
            let val = raw.parse::<i64>().map_err(|_| TraceError::Parse {
                line: line_num + 1,
                message: format!("Invalid cycle number: {}", raw),
            })?;
            current_cycle = val.max(0) as u32;
            continue;
        }

        if line == "C" || line.starts_with("C\t") {
            // "C" increments by 1; "C\t<n>" increments by n.
            if line.len() > 2 {
                let n = line[2..].trim().parse::<u32>().unwrap_or(1);
                current_cycle += n;
            } else {
                current_cycle += 1;
            }
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "I" => {
                if parts.len() < 4 {
                    return Err(TraceError::Parse {
                        line: line_num + 1,
                        message: "I command requires 4 fields".into(),
                    });
                }
                let id = parse_u32(parts[1], line_num)?;
                let sim_id = parse_u64(parts[2], line_num)?;
                let thread_id = parse_u16(parts[3], line_num)?;

                order.push(id);
                builders.insert(
                    id,
                    InstrBuilder {
                        id,
                        sim_id,
                        thread_id,
                        disasm: String::new(),
                        tooltip: String::new(),
                        stages: Vec::new(),
                        retire_status: RetireStatus::InFlight,
                        first_cycle: u32::MAX,
                        last_cycle: 0,
                        open_stages: HashMap::new(),
                    },
                );
            }
            "L" => {
                if parts.len() < 4 {
                    return Err(TraceError::Parse {
                        line: line_num + 1,
                        message: "L command requires 4 fields".into(),
                    });
                }
                let id = parse_u32(parts[1], line_num)?;
                let lane = parse_u8(parts[2], line_num)?;
                // Strip trailing literal "\n" that Konata uses as line break marker.
                let text = parts[3].trim_end_matches("\\n").trim_end_matches('\n');

                if let Some(builder) = builders.get_mut(&id) {
                    if lane == 0 {
                        // Lane 0: primary disassembly.
                        if !builder.disasm.is_empty() {
                            builder.disasm.push(' ');
                        }
                        builder.disasm.push_str(text);
                    } else {
                        // Lane >= 1: annotations → tooltip.
                        if !builder.tooltip.is_empty() {
                            builder.tooltip.push('\n');
                        }
                        builder.tooltip.push_str(text);
                    }
                }
            }
            "S" => {
                if parts.len() < 4 {
                    return Err(TraceError::Parse {
                        line: line_num + 1,
                        message: "S command requires 4 fields".into(),
                    });
                }
                let id = parse_u32(parts[1], line_num)?;
                let lane = parse_u8(parts[2], line_num)?;
                let stage_name = parts[3];

                let name_idx = trace.intern_stage(stage_name);

                if let Some(builder) = builders.get_mut(&id) {
                    // Close any previously open stage on this lane.
                    if let Some((prev_idx, start)) = builder.open_stages.remove(&lane) {
                        builder.stages.push(StageSpan {
                            stage_name_idx: prev_idx,
                            lane,
                            _pad: 0,
                            start_cycle: start,
                            end_cycle: current_cycle,
                        });
                        builder.first_cycle = builder.first_cycle.min(start);
                        builder.last_cycle = builder.last_cycle.max(current_cycle);
                    }
                    builder.open_stages.insert(lane, (name_idx, current_cycle));
                }
            }
            "E" => {
                if parts.len() < 4 {
                    return Err(TraceError::Parse {
                        line: line_num + 1,
                        message: "E command requires 4 fields".into(),
                    });
                }
                let id = parse_u32(parts[1], line_num)?;
                let lane = parse_u8(parts[2], line_num)?;

                if let Some(builder) = builders.get_mut(&id) {
                    if let Some((prev_idx, start)) = builder.open_stages.remove(&lane) {
                        let end = current_cycle;
                        builder.stages.push(StageSpan {
                            stage_name_idx: prev_idx,
                            lane,
                            _pad: 0,
                            start_cycle: start,
                            end_cycle: end,
                        });
                        builder.first_cycle = builder.first_cycle.min(start);
                        builder.last_cycle = builder.last_cycle.max(end);
                    }
                }
            }
            "R" => {
                if parts.len() < 4 {
                    return Err(TraceError::Parse {
                        line: line_num + 1,
                        message: "R command requires 4 fields".into(),
                    });
                }
                let id = parse_u32(parts[1], line_num)?;
                let retire_type = parse_u32(parts[3], line_num)?;

                if let Some(builder) = builders.get_mut(&id) {
                    // Close all open stages on all lanes.
                    for (lane, (prev_idx, start)) in builder.open_stages.drain() {
                        builder.stages.push(StageSpan {
                            stage_name_idx: prev_idx,
                            lane,
                            _pad: 0,
                            start_cycle: start,
                            end_cycle: current_cycle,
                        });
                        builder.first_cycle = builder.first_cycle.min(start);
                        builder.last_cycle = builder.last_cycle.max(current_cycle);
                    }
                    builder.retire_status = if retire_type == 0 {
                        RetireStatus::Retired
                    } else {
                        RetireStatus::Flushed
                    };
                }
            }
            "W" => {
                if parts.len() < 3 {
                    continue;
                }
                let consumer = parse_u32(parts[1], line_num)?;
                let producer = parse_u32(parts[2], line_num)?;
                trace.dependencies.push(Dependency {
                    producer,
                    consumer,
                    kind: DepKind::Data,
                });
            }
            _ => {
                // Unknown command, skip.
            }
        }
    }

    // Finalize: emit instructions in order.
    for id in &order {
        if let Some(mut builder) = builders.remove(id) {
            // Close any unclosed stages on all lanes.
            for (lane, (prev_idx, start)) in builder.open_stages.drain() {
                builder.stages.push(StageSpan {
                    stage_name_idx: prev_idx,
                    lane,
                    _pad: 0,
                    start_cycle: start,
                    end_cycle: current_cycle,
                });
                builder.first_cycle = builder.first_cycle.min(start);
                builder.last_cycle = builder.last_cycle.max(current_cycle);
            }

            let stage_start = trace.stages.len() as u32;
            trace.stages.extend(builder.stages);
            let stage_end = trace.stages.len() as u32;

            if builder.first_cycle == u32::MAX {
                builder.first_cycle = 0;
            }

            trace.add_instruction(InstructionData {
                id: builder.id,
                sim_id: builder.sim_id,
                thread_id: builder.thread_id,
                disasm: builder.disasm,
                tooltip: builder.tooltip,
                stage_range: stage_start..stage_end,
                retire_status: builder.retire_status,
                first_cycle: builder.first_cycle,
                last_cycle: builder.last_cycle,
            });
        }
    }

    Ok(trace)
}

fn parse_u32(s: &str, line: usize) -> Result<u32, TraceError> {
    s.parse().map_err(|_| TraceError::Parse {
        line: line + 1,
        message: format!("Expected u32, got '{}'", s),
    })
}

fn parse_u64(s: &str, line: usize) -> Result<u64, TraceError> {
    s.parse().map_err(|_| TraceError::Parse {
        line: line + 1,
        message: format!("Expected u64, got '{}'", s),
    })
}

fn parse_u16(s: &str, line: usize) -> Result<u16, TraceError> {
    s.parse().map_err(|_| TraceError::Parse {
        line: line + 1,
        message: format!("Expected u16, got '{}'", s),
    })
}

fn parse_u8(s: &str, line: usize) -> Result<u8, TraceError> {
    s.parse().map_err(|_| TraceError::Parse {
        line: line + 1,
        message: format!("Expected u8, got '{}'", s),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_trace() -> &'static str {
        "Kanata 0004\n\
         I\t0\t100\t0\n\
         L\t0\t0\tadd x1, x2, x3\n\
         S\t0\t0\tFetch\n\
         C\n\
         C\n\
         E\t0\t0\tFetch\n\
         S\t0\t0\tDecode\n\
         C\n\
         E\t0\t0\tDecode\n\
         R\t0\t100\t0\n"
    }

    #[test]
    fn test_minimal_trace() {
        let input = make_minimal_trace();
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        assert_eq!(trace.row_count(), 1);
        assert_eq!(trace.instructions[0].disasm, "add x1, x2, x3");
        assert_eq!(trace.instructions[0].retire_status, RetireStatus::Retired);

        let spans = trace.stages_for(0);
        assert_eq!(spans.len(), 2);
        assert_eq!(trace.stage_name(spans[0].stage_name_idx), "Fetch");
        assert_eq!(spans[0].start_cycle, 0);
        assert_eq!(spans[0].end_cycle, 2);
        assert_eq!(trace.stage_name(spans[1].stage_name_idx), "Decode");
    }

    #[test]
    fn test_flush() {
        let input = "Kanata 0004\n\
             I\t0\t100\t0\n\
             L\t0\t0\tnop\n\
             S\t0\t0\tFetch\n\
             C\n\
             E\t0\t0\tFetch\n\
             R\t0\t100\t1\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        assert_eq!(trace.instructions[0].retire_status, RetireStatus::Flushed);
    }

    #[test]
    fn test_dependencies() {
        let input = "Kanata 0004\n\
             I\t0\t100\t0\n\
             I\t1\t101\t0\n\
             L\t0\t0\tadd\n\
             L\t1\t0\tsub\n\
             W\t1\t0\n\
             S\t0\t0\tFetch\n\
             C\n\
             E\t0\t0\tFetch\n\
             R\t0\t100\t0\n\
             S\t1\t0\tFetch\n\
             C\n\
             E\t1\t0\tFetch\n\
             R\t1\t101\t0\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        assert_eq!(trace.dependencies.len(), 1);
        assert_eq!(trace.dependencies[0].producer, 0);
        assert_eq!(trace.dependencies[0].consumer, 1);
    }

    #[test]
    fn test_multi_lane() {
        let input = "Kanata 0004\n\
             I\t0\t100\t0\n\
             L\t0\t0\tnop\n\
             S\t0\t0\tFetch\n\
             C\n\
             E\t0\t0\tFetch\n\
             S\t0\t1\tMicroOp\n\
             C\n\
             E\t0\t1\tMicroOp\n\
             R\t0\t100\t0\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        let spans = trace.stages_for(0);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].lane, 0);
        assert_eq!(spans[1].lane, 1);
    }

    #[test]
    fn test_negative_cycle() {
        let input = "Kanata 0004\n\
             C=\t-1\n\
             C\t1\n\
             I\t0\t4\t0\n\
             L\t0\t1\tnop\n\
             S\t0\t0\tFetch\n\
             C\n\
             E\t0\t0\tFetch\n\
             R\t0\t4\t0\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        assert_eq!(trace.row_count(), 1);
        let spans = trace.stages_for(0);
        assert_eq!(spans[0].start_cycle, 1);
    }

    #[test]
    fn test_label_lanes_tooltip() {
        let input = "Kanata 0004\n\
             I\t0\t4\t0\n\
             L\t0\t0\tadd x1, x2, x3\n\
             L\t0\t1\t(g:4,c0)\\n\n\
             L\t0\t2\ti-cache-miss\\n\n\
             S\t0\t0\tFetch\n\
             C\n\
             E\t0\t0\tFetch\n\
             R\t0\t4\t0\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        assert_eq!(trace.row_count(), 1);
        assert_eq!(trace.instructions[0].disasm, "add x1, x2, x3");
        assert_eq!(trace.instructions[0].tooltip, "(g:4,c0)\ni-cache-miss");
    }

    #[test]
    fn test_relative_cycles() {
        let input = "Kanata 0004\n\
             C=10\n\
             I\t0\t100\t0\n\
             L\t0\t0\tnop\n\
             S\t0\t0\tFetch\n\
             C\n\
             C\n\
             E\t0\t0\tFetch\n\
             R\t0\t100\t0\n";
        let trace = parse_konata(&mut input.as_bytes()).unwrap();
        let spans = trace.stages_for(0);
        assert_eq!(spans[0].start_cycle, 10);
        assert_eq!(spans[0].end_cycle, 12); // E at cycle 12
    }
}
