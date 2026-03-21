use std::collections::HashMap;
use std::ops::Range;

/// Interned stage name index.
pub type StageNameIdx = u16;

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
pub struct QueueSlotEntry {
    /// Row index into `PipelineTrace::instructions`.
    pub row: usize,
    /// Current stage name index at the query cycle.
    pub stage: StageNameIdx,
    /// Whether the instruction is ready to issue (all operands available).
    pub is_ready: bool,
}

/// Queue occupancy at a specific cycle.
#[derive(Debug, Clone, Default)]
pub struct QueueOccupancy {
    /// Retire queue: indexed by RBID → Option<QueueSlotEntry>.
    pub retire_queue: Vec<Option<QueueSlotEntry>>,
    /// Dispatch queue entries grouped by queue ID.
    pub dispatch_queues: Vec<(u32, Vec<QueueSlotEntry>)>,
    /// Issue queue entries grouped by queue ID.
    pub issue_queues: Vec<(u32, Vec<QueueSlotEntry>)>,
}

/// The full pipeline trace — owns all data in SoA layout.
#[derive(Debug, Clone)]
pub struct PipelineTrace {
    pub instructions: Vec<InstructionData>,
    pub stages: Vec<StageSpan>,
    pub dependencies: Vec<Dependency>,
    /// Key-value metadata from the trace source (DUT properties, format info, etc.).
    pub metadata: Vec<(String, String)>,
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
            metadata: Vec::new(),
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
    pub fn stage_name_idx(&self, name: &str) -> Option<StageNameIdx> {
        self.stage_name_map.get(name).copied()
    }

    /// Compute queue occupancy at a given cycle.
    ///
    /// `retire_queue_size`: number of slots in the retire queue (e.g. 128).
    /// `issue_stages`: stage name indices that represent "in the issue queue".
    /// `retire_stages`: stage name indices that represent "in the retire queue".
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
            let mut current_stage = None;
            for span in stages {
                if span.start_cycle <= cycle && cycle < span.end_cycle {
                    current_stage = Some(span.stage_name_idx);
                    break;
                }
            }

            let stage = match current_stage {
                Some(s) => s,
                None => continue,
            };

            let is_ready = instr.ready_cycle.map(|rc| rc <= cycle).unwrap_or(false);

            // Check if in retire queue (any stage from Al through Cp).
            if retire_stages.contains(&stage) {
                if let Some(rbid) = instr.rbid {
                    let slot = rbid as usize % retire_queue.len().max(1);
                    retire_queue[slot] = Some(QueueSlotEntry {
                        row,
                        stage,
                        is_ready,
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
                });
            }

            // Check if in issue queue (Is stage).
            if issue_stages.contains(&stage) {
                let iq_id = instr.iq_id.unwrap_or(u32::MAX);
                iq_map.entry(iq_id).or_default().push(QueueSlotEntry {
                    row,
                    stage,
                    is_ready,
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
