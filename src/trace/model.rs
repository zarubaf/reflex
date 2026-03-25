use std::collections::HashMap;
use std::ops::Range;

/// Interned stage name index.
pub type StageNameIdx = u16;

/// Information about a buffer storage detected from the uscope schema.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BufferInfo {
    pub name: String,
    pub storage_id: u16,
    pub capacity: u16,
    /// Fields defined on this buffer: (name, field_type as u8).
    pub fields: Vec<(String, u8)>,
}

/// A single stage span within an instruction's pipeline execution.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct StageSpan {
    pub stage_name_idx: StageNameIdx,
    pub lane: u8,
    pub _pad: u8,
    pub start_cycle: u32,
    pub end_cycle: u32,
}

static_assertions_size!(StageSpan, 12);

/// Dependency relationship between two instructions.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Dependency {
    pub producer: u32,
    pub consumer: u32,
    pub kind: DepKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Data,
    Control,
    Memory,
}

/// Retirement status of an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetireStatus {
    Retired,
    Flushed,
    InFlight,
}

/// Per-instruction data.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InstructionData {
    pub id: u32,
    pub sim_id: u64,
    pub thread_id: u16,
    /// Retire buffer ID (slot in the retire queue). `None` if not yet allocated.
    pub rbid: Option<u32>,
    /// Issue queue ID (index into cpu.issue_queue_names). `None` if unknown.
    pub iq_id: Option<u32>,
    /// Dispatch queue ID (index into cpu.dispatch_queue_names). `None` if unknown.
    pub dq_id: Option<u32>,
    /// Cycle at which the instruction became ready in the issue queue. `None` if not yet ready.
    pub ready_cycle: Option<u32>,
    pub disasm: String,
    pub tooltip: String,
    pub stage_range: Range<u32>,
    pub retire_status: RetireStatus,
    pub first_cycle: u32,
    pub last_cycle: u32,
}

/// Occupancy snapshot of a queue at a specific cycle.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct QueueSlotEntry {
    /// Row index into `PipelineTrace::instructions`.
    pub row: usize,
    /// Current stage name index at the query cycle.
    pub stage: StageNameIdx,
    /// Whether the instruction is ready to issue (all operands available).
    pub is_ready: bool,
    /// Cycle when the instruction entered this stage.
    pub stage_start_cycle: u32,
}

/// Queue occupancy at a specific cycle.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct QueueOccupancy {
    /// Retire queue: indexed by RBID → Option<QueueSlotEntry>.
    pub retire_queue: Vec<Option<QueueSlotEntry>>,
    /// Dispatch queue entries grouped by queue ID.
    pub dispatch_queues: Vec<(u32, Vec<QueueSlotEntry>)>,
    /// Issue queue entries grouped by queue ID.
    pub issue_queues: Vec<(u32, Vec<QueueSlotEntry>)>,
}

/// Display mode for a performance counter value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CounterDisplayMode {
    /// Raw cumulative value.
    Total,
    /// Delta / window_size (e.g., IPC).
    Rate,
    /// Single-cycle change.
    Delta,
}

/// A single performance counter time-series.
#[derive(Debug, Clone)]
pub struct CounterSeries {
    /// Display name (scope-qualified if multi-scope trace).
    pub name: String,
    /// Cumulative values indexed by cycle (0-based from trace start).
    /// `values[cycle]` = total count at end of that cycle.
    pub values: Vec<u64>,
    /// Default display mode.
    pub default_mode: CounterDisplayMode,
}

/// The full pipeline trace — owns all data in SoA layout.
#[derive(Debug, Clone)]
pub struct PipelineTrace {
    pub instructions: Vec<InstructionData>,
    pub stages: Vec<StageSpan>,
    pub dependencies: Vec<Dependency>,
    /// Performance counter time-series parsed from uscope traces.
    pub counters: Vec<CounterSeries>,
    /// Buffer storages detected from uscope schema (SF_BUFFER flag).
    pub buffers: Vec<BufferInfo>,
    /// Key-value metadata from the trace source (DUT properties, format info, etc.).
    pub metadata: Vec<(String, String)>,
    /// Clock period in picoseconds (from uscope traces). Enables cycle↔timestamp conversion.
    pub period_ps: Option<u64>,
    stage_names: Vec<String>,
    stage_name_map: HashMap<String, StageNameIdx>,
    id_to_row: HashMap<u32, usize>,
}

impl PipelineTrace {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            stages: Vec::new(),
            dependencies: Vec::new(),
            counters: Vec::new(),
            buffers: Vec::new(),
            metadata: Vec::new(),
            period_ps: None,
            stage_names: Vec::new(),
            stage_name_map: HashMap::new(),
            id_to_row: HashMap::new(),
        }
    }

    /// Intern a stage name, returning its index.
    pub fn intern_stage(&mut self, name: &str) -> StageNameIdx {
        if let Some(&idx) = self.stage_name_map.get(name) {
            return idx;
        }
        let idx = self.stage_names.len() as StageNameIdx;
        self.stage_names.push(name.to_string());
        self.stage_name_map.insert(name.to_string(), idx);
        idx
    }

    /// Look up a stage name by index.
    pub fn stage_name(&self, idx: StageNameIdx) -> &str {
        &self.stage_names[idx as usize]
    }

    /// Number of interned stage names.
    #[allow(dead_code)]
    pub fn stage_name_count(&self) -> usize {
        self.stage_names.len()
    }

    /// Add an instruction and update the id→row mapping.
    pub fn add_instruction(&mut self, instr: InstructionData) {
        let row = self.instructions.len();
        self.id_to_row.insert(instr.id, row);
        self.instructions.push(instr);
    }

    /// Get row index for an instruction id.
    #[allow(dead_code)]
    pub fn row_for_id(&self, id: u32) -> Option<usize> {
        self.id_to_row.get(&id).copied()
    }

    /// Get stage spans for a given row.
    pub fn stages_for(&self, row: usize) -> &[StageSpan] {
        let instr = &self.instructions[row];
        &self.stages[instr.stage_range.start as usize..instr.stage_range.end as usize]
    }

    /// Total number of instructions/rows.
    pub fn row_count(&self) -> usize {
        self.instructions.len()
    }

    /// The maximum cycle across all stages (for viewport bounds).
    pub fn max_cycle(&self) -> u32 {
        self.instructions
            .iter()
            .map(|i| i.last_cycle)
            .max()
            .unwrap_or(0)
    }

    /// Look up a stage name index by name, if it exists.
    #[allow(dead_code)]
    pub fn stage_name_idx(&self, name: &str) -> Option<StageNameIdx> {
        self.stage_name_map.get(name).copied()
    }

    /// Get cumulative counter value at a cycle.
    pub fn counter_value_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        let series = &self.counters[counter_idx];
        if series.values.is_empty() {
            return 0;
        }
        let idx = (cycle as usize).min(series.values.len() - 1);
        series.values[idx]
    }

    /// Get counter rate over a window ending at the given cycle.
    pub fn counter_rate_at(&self, counter_idx: usize, cycle: u32, window: u32) -> f64 {
        let end_val = self.counter_value_at(counter_idx, cycle);
        let start_cycle = cycle.saturating_sub(window);
        let start_val = self.counter_value_at(counter_idx, start_cycle);
        let actual_window = cycle.saturating_sub(start_cycle);
        if actual_window == 0 {
            return 0.0;
        }
        (end_val.wrapping_sub(start_val)) as f64 / actual_window as f64
    }

    /// Get single-cycle delta for a counter.
    pub fn counter_delta_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        let curr = self.counter_value_at(counter_idx, cycle);
        let prev = if cycle > 0 {
            self.counter_value_at(counter_idx, cycle - 1)
        } else {
            0
        };
        curr.wrapping_sub(prev)
    }

    /// Downsample a counter's per-cycle deltas to min-max envelope buckets.
    ///
    /// Returns `bucket_count` pairs of `(min_delta, max_delta)` covering
    /// `[start_cycle, end_cycle)`. Each bucket aggregates the deltas
    /// (single-cycle changes) within its range. Useful for sparkline rendering
    /// where many cycles compress into one pixel.
    pub fn counter_downsample_minmax(
        &self,
        counter_idx: usize,
        start_cycle: u32,
        end_cycle: u32,
        bucket_count: usize,
    ) -> Vec<(u64, u64)> {
        if bucket_count == 0 || start_cycle >= end_cycle {
            return Vec::new();
        }
        let series = &self.counters[counter_idx];
        if series.values.is_empty() {
            return vec![(0, 0); bucket_count];
        }
        let range = end_cycle.saturating_sub(start_cycle) as f64;
        let cycles_per_bucket = range / bucket_count as f64;

        let mut result = Vec::with_capacity(bucket_count);
        for b in 0..bucket_count {
            let bucket_start = start_cycle + (b as f64 * cycles_per_bucket) as u32;
            let bucket_end = start_cycle + ((b + 1) as f64 * cycles_per_bucket) as u32;
            let bucket_end = bucket_end.min(end_cycle);

            let mut min_delta = u64::MAX;
            let mut max_delta = 0u64;
            for cy in bucket_start..bucket_end {
                let delta = self.counter_delta_at(counter_idx, cy);
                min_delta = min_delta.min(delta);
                max_delta = max_delta.max(delta);
            }
            if min_delta == u64::MAX {
                min_delta = 0;
            }
            result.push((min_delta, max_delta));
        }
        result
    }

    /// Compute queue occupancy at a given cycle.
    ///
    /// `retire_queue_size`: number of slots in the retire queue (e.g. 128).
    /// `issue_stages`: stage name indices that represent "in the issue queue".
    /// `retire_stages`: stage name indices that represent "in the retire queue".
    #[allow(dead_code)]
    pub fn queue_occupancy_at(
        &self,
        cycle: u32,
        retire_queue_size: usize,
        dispatch_stages: &[StageNameIdx],
        issue_stages: &[StageNameIdx],
        retire_stages: &[StageNameIdx],
    ) -> QueueOccupancy {
        let mut retire_queue = vec![None; retire_queue_size];
        let mut dq_map: HashMap<u32, Vec<QueueSlotEntry>> = HashMap::new();
        let mut iq_map: HashMap<u32, Vec<QueueSlotEntry>> = HashMap::new();

        for (row, instr) in self.instructions.iter().enumerate() {
            if instr.first_cycle > cycle || instr.last_cycle < cycle {
                continue;
            }

            // Find which stage this instruction is in at `cycle`.
            let stages =
                &self.stages[instr.stage_range.start as usize..instr.stage_range.end as usize];
            let mut current_span = None;
            for span in stages {
                if span.start_cycle <= cycle && cycle < span.end_cycle {
                    current_span = Some(span);
                    break;
                }
            }

            let span = match current_span {
                Some(s) => s,
                None => continue,
            };
            let stage = span.stage_name_idx;
            let stage_start_cycle = span.start_cycle;

            let is_ready = instr.ready_cycle.map(|rc| rc <= cycle).unwrap_or(false);

            // Check if in retire queue (any stage from Al through Cp).
            if retire_stages.contains(&stage) {
                if let Some(rbid) = instr.rbid {
                    let slot = rbid as usize % retire_queue.len().max(1);
                    retire_queue[slot] = Some(QueueSlotEntry {
                        row,
                        stage,
                        is_ready,
                        stage_start_cycle,
                    });
                }
            }

            // Check if in dispatch queue (Ds stage).
            if dispatch_stages.contains(&stage) {
                let dq_id = instr.dq_id.unwrap_or(u32::MAX);
                dq_map.entry(dq_id).or_default().push(QueueSlotEntry {
                    row,
                    stage,
                    is_ready,
                    stage_start_cycle,
                });
            }

            // Check if in issue queue (Is stage).
            if issue_stages.contains(&stage) {
                let iq_id = instr.iq_id.unwrap_or(u32::MAX);
                iq_map.entry(iq_id).or_default().push(QueueSlotEntry {
                    row,
                    stage,
                    is_ready,
                    stage_start_cycle,
                });
            }
        }

        let mut dispatch_queues: Vec<_> = dq_map.into_iter().collect();
        dispatch_queues.sort_by_key(|(id, _)| *id);

        let mut issue_queues: Vec<_> = iq_map.into_iter().collect();
        issue_queues.sort_by_key(|(id, _)| *id);

        QueueOccupancy {
            retire_queue,
            dispatch_queues,
            issue_queues,
        }
    }

    /// Rebuild the id→row mapping (e.g. after deserialization).
    #[allow(dead_code)]
    pub fn rebuild_id_map(&mut self) {
        self.id_to_row.clear();
        for (row, instr) in self.instructions.iter().enumerate() {
            self.id_to_row.insert(instr.id, row);
        }
    }
}

impl Default for PipelineTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Compile-time size assertion helper (no-op, just for documentation).
macro_rules! static_assertions_size {
    ($t:ty, $expected:expr) => {
        #[cfg(test)]
        const _: () = {
            assert!(std::mem::size_of::<$t>() <= $expected);
        };
    };
}
use static_assertions_size;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_stage_names() {
        let mut trace = PipelineTrace::new();
        let fetch = trace.intern_stage("Fetch");
        let decode = trace.intern_stage("Decode");
        let fetch2 = trace.intern_stage("Fetch");
        assert_eq!(fetch, fetch2);
        assert_ne!(fetch, decode);
        assert_eq!(trace.stage_name(fetch), "Fetch");
        assert_eq!(trace.stage_name(decode), "Decode");
        assert_eq!(trace.stage_name_count(), 2);
    }

    #[test]
    fn test_row_for_id() {
        let mut trace = PipelineTrace::new();
        trace.add_instruction(InstructionData {
            id: 42,
            sim_id: 100,
            thread_id: 0,
            rbid: None,
            iq_id: None,
            dq_id: None,
            ready_cycle: None,
            disasm: "add x1, x2, x3".to_string(),
            tooltip: String::new(),
            stage_range: 0..0,
            retire_status: RetireStatus::Retired,
            first_cycle: 0,
            last_cycle: 5,
        });
        trace.add_instruction(InstructionData {
            id: 43,
            sim_id: 101,
            thread_id: 0,
            rbid: None,
            iq_id: None,
            dq_id: None,
            ready_cycle: None,
            disasm: "sub x4, x5, x6".to_string(),
            tooltip: String::new(),
            stage_range: 0..0,
            retire_status: RetireStatus::Retired,
            first_cycle: 1,
            last_cycle: 6,
        });
        assert_eq!(trace.row_for_id(42), Some(0));
        assert_eq!(trace.row_for_id(43), Some(1));
        assert_eq!(trace.row_for_id(99), None);
    }

    #[test]
    fn test_counter_value_at() {
        let mut trace = PipelineTrace::new();
        trace.counters.push(CounterSeries {
            name: "committed_insns".to_string(),
            // Cycles: 0=0, 1=2, 2=5, 3=5, 4=8
            values: vec![0, 2, 5, 5, 8],
            default_mode: CounterDisplayMode::Total,
        });
        assert_eq!(trace.counter_value_at(0, 0), 0);
        assert_eq!(trace.counter_value_at(0, 2), 5);
        assert_eq!(trace.counter_value_at(0, 4), 8);
        // Beyond range clamps to last value
        assert_eq!(trace.counter_value_at(0, 100), 8);
    }

    #[test]
    fn test_counter_rate_at() {
        let mut trace = PipelineTrace::new();
        trace.counters.push(CounterSeries {
            name: "committed_insns".to_string(),
            values: vec![0, 2, 4, 6, 8, 10],
            default_mode: CounterDisplayMode::Rate,
        });
        // Rate over 2-cycle window at cycle 4: (8-4)/2 = 2.0
        assert!((trace.counter_rate_at(0, 4, 2) - 2.0).abs() < f64::EPSILON);
        // Rate at cycle 0 with window 2: (0-0)/0 = 0 (edge case)
        assert!((trace.counter_rate_at(0, 0, 2) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_counter_delta_at() {
        let mut trace = PipelineTrace::new();
        trace.counters.push(CounterSeries {
            name: "bp_misses".to_string(),
            values: vec![0, 0, 1, 1, 3],
            default_mode: CounterDisplayMode::Total,
        });
        assert_eq!(trace.counter_delta_at(0, 0), 0);
        assert_eq!(trace.counter_delta_at(0, 2), 1);
        assert_eq!(trace.counter_delta_at(0, 3), 0);
        assert_eq!(trace.counter_delta_at(0, 4), 2);
    }

    #[test]
    fn test_counter_downsample_minmax() {
        let mut trace = PipelineTrace::new();
        // Cumulative: 0, 2, 5, 5, 8, 10, 10, 13, 15, 20
        // Deltas:     0, 2, 3, 0, 3,  2,  0,  3,  2,  5
        trace.counters.push(CounterSeries {
            name: "test".to_string(),
            values: vec![0, 2, 5, 5, 8, 10, 10, 13, 15, 20],
            default_mode: CounterDisplayMode::Total,
        });

        // 2 buckets over 10 cycles: bucket 0 = cycles 0..5, bucket 1 = cycles 5..10
        let result = trace.counter_downsample_minmax(0, 0, 10, 2);
        assert_eq!(result.len(), 2);
        // Bucket 0 deltas: 0, 2, 3, 0, 3 → min=0, max=3
        assert_eq!(result[0], (0, 3));
        // Bucket 1 deltas: 2, 0, 3, 2, 5 → min=0, max=5
        assert_eq!(result[1], (0, 5));

        // Edge case: empty range
        assert_eq!(trace.counter_downsample_minmax(0, 5, 5, 10).len(), 0);
        // Edge case: zero buckets
        assert_eq!(trace.counter_downsample_minmax(0, 0, 10, 0).len(), 0);
    }

    #[test]
    fn test_stages_for() {
        let mut trace = PipelineTrace::new();
        let fetch = trace.intern_stage("Fetch");
        let decode = trace.intern_stage("Decode");

        trace.stages.push(StageSpan {
            stage_name_idx: fetch,
            lane: 0,
            _pad: 0,
            start_cycle: 0,
            end_cycle: 2,
        });
        trace.stages.push(StageSpan {
            stage_name_idx: decode,
            lane: 0,
            _pad: 0,
            start_cycle: 2,
            end_cycle: 4,
        });

        trace.add_instruction(InstructionData {
            id: 0,
            sim_id: 0,
            thread_id: 0,
            rbid: None,
            iq_id: None,
            dq_id: None,
            ready_cycle: None,
            disasm: "nop".to_string(),
            tooltip: String::new(),
            stage_range: 0..2,
            retire_status: RetireStatus::Retired,
            first_cycle: 0,
            last_cycle: 4,
        });

        let spans = trace.stages_for(0);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].stage_name_idx, fetch);
        assert_eq!(spans[1].stage_name_idx, decode);
    }
}
