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
    /// Sparse counter samples: (cycle, cumulative_value) pairs.
    /// One entry per segment boundary (from checkpoint data).
    /// Sorted by cycle.
    pub samples: Vec<(u32, u64)>,
    /// Default display mode.
    pub default_mode: CounterDisplayMode,
}

/// Lightweight index mapping segment indices to their cycle ranges.
/// Built on load from uscope segment time bounds; enables binary search
/// for "which segments cover cycles N..M?" in future lazy-loading phases.
#[derive(Debug, Clone, Default)]
pub struct SegmentIndex {
    /// (start_cycle, end_cycle) per segment, ordered by segment index.
    pub segments: Vec<(u32, u32)>,
}

impl SegmentIndex {
    /// Find segment indices that overlap the given cycle range.
    #[allow(dead_code)]
    pub fn segments_in_range(&self, start_cycle: u32, end_cycle: u32) -> Vec<usize> {
        self.segments
            .iter()
            .enumerate()
            .filter(|(_, (seg_start, seg_end))| *seg_start < end_cycle && *seg_end > start_cycle)
            .map(|(idx, _)| idx)
            .collect()
    }
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
    /// Total instruction count in the trace (exact, from file metadata or counting).
    /// May differ from `instructions.len()` during lazy loading.
    pub total_instruction_count: usize,
    /// Global instruction index: (first_cycle, entity_id) for every instruction in the trace.
    /// Built during open, enables row-to-cycle mapping for lazy loading.
    /// Sorted by first_cycle.
    pub instruction_index: Vec<(u32, u32)>,
    /// Key-value metadata from the trace source (DUT properties, format info, etc.).
    pub metadata: Vec<(String, String)>,
    /// Clock period in picoseconds (from uscope traces). Enables cycle↔timestamp conversion.
    pub period_ps: Option<u64>,
    /// If set, `max_cycle()` returns this value instead of computing from instructions.
    /// Used by lazy-loading to report the full trace extent before instructions are loaded.
    pub max_cycle_override: Option<u32>,
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
            total_instruction_count: 0,
            instruction_index: Vec::new(),
            metadata: Vec::new(),
            period_ps: None,
            max_cycle_override: None,
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
    /// When `max_cycle_override` is set (lazy-loading mode), returns that
    /// value so the viewport knows the full trace extent even before
    /// all instructions are loaded.
    pub fn max_cycle(&self) -> u32 {
        if let Some(ov) = self.max_cycle_override {
            return ov;
        }
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
    ///
    /// Uses binary search over sparse samples. Returns the value from the
    /// sample at or just before the given cycle.
    pub fn counter_value_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        let series = &self.counters[counter_idx];
        if series.samples.is_empty() {
            return 0;
        }
        match series.samples.binary_search_by_key(&cycle, |(c, _)| *c) {
            Ok(i) => series.samples[i].1,
            Err(0) => 0,
            Err(i) => series.samples[i - 1].1,
        }
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
    ///
    /// With sparse samples, computes the interpolated per-cycle rate between
    /// the two nearest samples surrounding the given cycle.
    pub fn counter_delta_at(&self, counter_idx: usize, cycle: u32) -> u64 {
        let series = &self.counters[counter_idx];
        if series.samples.is_empty() {
            return 0;
        }
        // Find the sample interval containing this cycle and compute
        // the average per-cycle delta within that interval.
        match series.samples.binary_search_by_key(&cycle, |(c, _)| *c) {
            Ok(i) => {
                // Exact match on a sample boundary.
                if i == 0 {
                    // First sample: delta from 0 to this value over the cycles.
                    let (c, v) = series.samples[0];
                    if c == 0 {
                        return v;
                    }
                    return v / c as u64;
                }
                let (prev_c, prev_v) = series.samples[i - 1];
                let (cur_c, cur_v) = series.samples[i];
                let span = cur_c.saturating_sub(prev_c) as u64;
                if span == 0 {
                    return 0;
                }
                cur_v.wrapping_sub(prev_v) / span
            }
            Err(0) => {
                // Before the first sample.
                if series.samples.is_empty() {
                    return 0;
                }
                let (c, v) = series.samples[0];
                if c == 0 {
                    return 0;
                }
                v / c as u64
            }
            Err(i) if i >= series.samples.len() => {
                // After the last sample: assume the counter stops changing.
                0
            }
            Err(i) => {
                // Between samples[i-1] and samples[i].
                let (prev_c, prev_v) = series.samples[i - 1];
                let (next_c, next_v) = series.samples[i];
                let span = next_c.saturating_sub(prev_c) as u64;
                if span == 0 {
                    return 0;
                }
                next_v.wrapping_sub(prev_v) / span
            }
        }
    }

    /// Downsample a counter to min-max envelope buckets over a cycle range.
    ///
    /// Returns `bucket_count` pairs of `(min_rate, max_rate)` covering
    /// `[start_cycle, end_cycle)`. Each bucket reports the min and max
    /// per-cycle rates among the sparse sample intervals that overlap
    /// that bucket. Useful for sparkline rendering where many cycles
    /// compress into one pixel.
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
        if series.samples.is_empty() {
            return vec![(0, 0); bucket_count];
        }

        // Build a list of (interval_start_cycle, interval_end_cycle, per_cycle_rate)
        // from the sparse samples. The first interval runs from cycle 0 to samples[0].
        let mut intervals: Vec<(u32, u32, u64)> = Vec::with_capacity(series.samples.len() + 1);
        if let Some(&(first_c, first_v)) = series.samples.first() {
            if first_c > 0 {
                let rate = if first_c > 0 {
                    first_v / first_c as u64
                } else {
                    0
                };
                intervals.push((0, first_c, rate));
            }
        }
        for w in series.samples.windows(2) {
            let (c0, v0) = w[0];
            let (c1, v1) = w[1];
            let span = c1.saturating_sub(c0);
            let rate = if span > 0 {
                v1.wrapping_sub(v0) / span as u64
            } else {
                0
            };
            intervals.push((c0, c1, rate));
        }

        let range = end_cycle.saturating_sub(start_cycle) as f64;
        let cycles_per_bucket = range / bucket_count as f64;

        let mut result = Vec::with_capacity(bucket_count);
        for b in 0..bucket_count {
            let bucket_start = start_cycle + (b as f64 * cycles_per_bucket) as u32;
            let bucket_end = start_cycle + ((b + 1) as f64 * cycles_per_bucket) as u32;
            let bucket_end = bucket_end.min(end_cycle);

            let mut min_rate = u64::MAX;
            let mut max_rate = 0u64;

            // Find intervals that overlap [bucket_start, bucket_end).
            for &(iv_start, iv_end, rate) in &intervals {
                if iv_start < bucket_end && iv_end > bucket_start {
                    min_rate = min_rate.min(rate);
                    max_rate = max_rate.max(rate);
                }
            }
            if min_rate == u64::MAX {
                // No intervals overlap this bucket — use interpolated value.
                let rate = self.counter_delta_at(counter_idx, bucket_start);
                min_rate = rate;
                max_rate = rate;
            }
            result.push((min_rate, max_rate));
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

    /// Merge instructions, stages, and dependencies from a segment load.
    ///
    /// The incoming `stage_range` values are relative to the incoming `stages`
    /// slice. This method offsets them to match the existing `self.stages` vec,
    /// appends everything, then re-sorts instructions by `first_cycle` and
    /// rebuilds the `id_to_row` map.
    pub fn merge_loaded(
        &mut self,
        mut instructions: Vec<InstructionData>,
        stages: Vec<StageSpan>,
        dependencies: Vec<Dependency>,
    ) {
        let stage_offset = self.stages.len() as u32;
        for instr in &mut instructions {
            instr.stage_range =
                (instr.stage_range.start + stage_offset)..(instr.stage_range.end + stage_offset);
        }
        self.stages.extend(stages);
        self.dependencies.extend(dependencies);
        self.instructions.extend(instructions);

        // Re-sort all instructions by first_cycle for correct rendering order.
        // This is needed because newly loaded segments may interleave with
        // previously loaded ones.
        self.instructions
            .sort_by(|a, b| a.first_cycle.cmp(&b.first_cycle).then(a.id.cmp(&b.id)));

        // Rebuild id→row map after re-sort.
        self.rebuild_id_map();
    }

    /// Rebuild the id→row mapping (e.g. after deserialization or merge).
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
        // Sparse samples: (cycle, cumulative_value)
        trace.counters.push(CounterSeries {
            name: "committed_insns".to_string(),
            samples: vec![(0, 0), (2, 5), (4, 8)],
            default_mode: CounterDisplayMode::Total,
        });
        assert_eq!(trace.counter_value_at(0, 0), 0);
        assert_eq!(trace.counter_value_at(0, 2), 5);
        assert_eq!(trace.counter_value_at(0, 4), 8);
        // Between samples: uses previous sample
        assert_eq!(trace.counter_value_at(0, 1), 0);
        assert_eq!(trace.counter_value_at(0, 3), 5);
        // Beyond range: uses last sample
        assert_eq!(trace.counter_value_at(0, 100), 8);
    }

    #[test]
    fn test_counter_rate_at() {
        let mut trace = PipelineTrace::new();
        // Linear: 2 per cycle
        trace.counters.push(CounterSeries {
            name: "committed_insns".to_string(),
            samples: vec![(0, 0), (2, 4), (4, 8)],
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
        // Sparse samples at segment boundaries.
        // Between cycle 0-2: cumulative goes 0→4, so avg rate = 4/2 = 2 per cycle.
        // Between cycle 2-4: cumulative goes 4→10, so avg rate = 6/2 = 3 per cycle.
        trace.counters.push(CounterSeries {
            name: "bp_misses".to_string(),
            samples: vec![(0, 0), (2, 4), (4, 10)],
            default_mode: CounterDisplayMode::Total,
        });
        // At cycle 0 (exact match, first sample): delta is 0 (value/cycle=0/0)
        assert_eq!(trace.counter_delta_at(0, 0), 0);
        // At cycle 1 (between 0 and 2): avg rate = 4/2 = 2
        assert_eq!(trace.counter_delta_at(0, 1), 2);
        // At cycle 3 (between 2 and 4): avg rate = 6/2 = 3
        assert_eq!(trace.counter_delta_at(0, 3), 3);
    }

    #[test]
    fn test_counter_downsample_minmax() {
        let mut trace = PipelineTrace::new();
        // Sparse samples with varying rates between segments.
        // (0,0)→(5,10): rate=2/cycle, (5,10)→(10,20): rate=2/cycle
        // But let's make it more interesting:
        // (0,0)→(5,5): rate=1/cycle, (5,5)→(10,20): rate=3/cycle
        trace.counters.push(CounterSeries {
            name: "test".to_string(),
            samples: vec![(0, 0), (5, 5), (10, 20)],
            default_mode: CounterDisplayMode::Total,
        });

        // 2 buckets over 10 cycles: bucket 0 = cycles 0..5, bucket 1 = cycles 5..10
        let result = trace.counter_downsample_minmax(0, 0, 10, 2);
        assert_eq!(result.len(), 2);
        // Bucket 0 covers interval (0,5) with rate=1 → min=1, max=1
        assert_eq!(result[0], (1, 1));
        // Bucket 1 covers interval (5,10) with rate=3 → min=3, max=3
        assert_eq!(result[1], (3, 3));

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
