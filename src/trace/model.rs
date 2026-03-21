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
    pub disasm: String,
    pub tooltip: String,
    pub stage_range: Range<u32>,
    pub retire_status: RetireStatus,
    pub first_cycle: u32,
    pub last_cycle: u32,
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
