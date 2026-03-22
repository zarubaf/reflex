use crate::trace::model::{
    DepKind, Dependency, InstructionData, PipelineTrace, RetireStatus, StageSpan,
};
use crate::trace::{TraceError, TraceSource};
use instruction_decoder::Decoder;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use uscope::reader::Reader;
use uscope::state::TimedItem;
use uscope::types::*;

pub struct UscopeSource;

impl TraceSource for UscopeSource {
    fn format_name(&self) -> &str {
        "µScope"
    }

    fn file_extensions(&self) -> &[&str] {
        &["uscope"]
    }

    fn detect(&self, first_bytes: &[u8]) -> bool {
        first_bytes.len() >= 4 && first_bytes[..4] == MAGIC
    }

    fn load(&self, _reader: &mut dyn Read) -> Result<PipelineTrace, TraceError> {
        Err(TraceError::UnsupportedFormat(
            "uscope format requires file path for seeking; use load_file()".into(),
        ))
    }

    fn load_file(&self, path: &Path) -> Result<PipelineTrace, TraceError> {
        parse_uscope(path)
    }
}

/// Resolved CPU protocol IDs from the schema.
struct CpuProtocolIds {
    period_ps: u64,
    entities_storage_id: u16,
    field_entity_id: u16,
    field_pc: u16,
    field_inst_bits: Option<u16>,
    field_rbid: Option<u16>,
    field_iq_id: Option<u16>,
    field_dq_id: Option<u16>,
    field_ready_time_ps: Option<u16>,
    stage_transition_event_id: u16,
    annotate_event_id: u16,
    dependency_event_id: u16,
    flush_event_id: u16,
    stage_names: Vec<String>,
}

fn resolve_cpu_protocol(reader: &Reader) -> Result<CpuProtocolIds, TraceError> {
    let schema = reader.schema();

    // Find scope with protocol == "cpu"
    let cpu_scope = schema
        .scopes
        .iter()
        .find(|s| schema.get_string(s.protocol) == Some("cpu"))
        .ok_or_else(|| TraceError::UnsupportedFormat("no CPU protocol scope found".into()))?;

    // Get clock period
    let clock_id = cpu_scope.clock_id;
    let period_ps = schema
        .clock_domains
        .get(clock_id as usize)
        .map(|cd| cd.period_ps as u64)
        .unwrap_or(1000); // default 1 GHz

    // Find entities storage (name == "entities" in this scope)
    let entities_storage = schema
        .storages
        .iter()
        .find(|s| s.scope_id == cpu_scope.scope_id && schema.get_string(s.name) == Some("entities"))
        .ok_or_else(|| TraceError::UnsupportedFormat("no entities storage found".into()))?;

    // Find field indices by name
    let field_entity_id = find_field_index(schema, entities_storage, "entity_id")?;
    let field_pc = find_field_index(schema, entities_storage, "pc")?;
    let field_inst_bits = find_field_index(schema, entities_storage, "inst_bits").ok();
    let field_rbid = find_field_index(schema, entities_storage, "rbid").ok();
    let field_iq_id = find_field_index(schema, entities_storage, "iq_id").ok();
    let field_dq_id = find_field_index(schema, entities_storage, "dq_id").ok();
    let field_ready_time_ps = find_field_index(schema, entities_storage, "ready_time_ps").ok();

    // Find events by name in the CPU scope
    let find_event = |name: &str| -> Result<u16, TraceError> {
        schema
            .events
            .iter()
            .find(|e| e.scope_id == cpu_scope.scope_id && schema.get_string(e.name) == Some(name))
            .map(|e| e.event_type_id)
            .ok_or_else(|| TraceError::UnsupportedFormat(format!("no '{}' event found", name)))
    };

    let stage_transition_event_id = find_event("stage_transition")?;
    let annotate_event_id = find_event("annotate")?;
    let dependency_event_id = find_event("dependency")?;
    let flush_event_id = find_event("flush")?;

    // Read pipeline stage names from the pipeline_stage enum
    let stage_names = read_stage_names(reader)?;

    Ok(CpuProtocolIds {
        period_ps,
        entities_storage_id: entities_storage.storage_id,
        field_entity_id,
        field_pc,
        field_inst_bits,
        field_rbid,
        field_iq_id,
        field_dq_id,
        field_ready_time_ps,
        stage_transition_event_id,
        annotate_event_id,
        dependency_event_id,
        flush_event_id,
        stage_names,
    })
}

fn find_field_index(
    schema: &uscope::schema::Schema,
    storage: &StorageDef,
    name: &str,
) -> Result<u16, TraceError> {
    storage
        .fields
        .iter()
        .position(|f| schema.get_string(f.name) == Some(name))
        .map(|i| i as u16)
        .ok_or_else(|| {
            TraceError::UnsupportedFormat(format!("field '{}' not found in entities", name))
        })
}

fn read_stage_names(reader: &Reader) -> Result<Vec<String>, TraceError> {
    // Try DUT property first (canonical ordering)
    if let Some(stages_str) = reader.dut_property("cpu.pipeline_stages") {
        let names: Vec<String> = stages_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        if !names.is_empty() {
            return Ok(names);
        }
    }

    // Fallback: read from enum
    let schema = reader.schema();
    let pipeline_enum = schema
        .enums
        .iter()
        .find(|e| schema.get_string(e.name) == Some("pipeline_stage"))
        .ok_or_else(|| TraceError::UnsupportedFormat("no pipeline_stage enum found".into()))?;

    let mut names: Vec<String> = pipeline_enum
        .values
        .iter()
        .map(|v| schema.get_string(v.name).unwrap_or("?").to_string())
        .collect();

    if names.is_empty() {
        names.push("unknown".to_string());
    }

    Ok(names)
}

/// Transient builder for an instruction being reconstructed.
struct InstrBuilder {
    entity_id: u32,
    reflex_id: u32,
    pc: u64,
    inst_bits: Option<u32>,
    rbid: Option<u32>,
    iq_id: Option<u32>,
    dq_id: Option<u32>,
    ready_time_ps: Option<u64>,
    has_disasm_annotation: bool,
    disasm: String,
    tooltip: String,
    stages: Vec<StageSpan>,
    current_stage: Option<(u16, u32)>, // (stage_name_idx, start_cycle)
    first_cycle: u32,
    last_cycle: u32,
    retire_status: RetireStatus,
}

impl InstrBuilder {
    fn new(entity_id: u32, reflex_id: u32, pc: u64, cycle: u32) -> Self {
        Self {
            entity_id,
            reflex_id,
            pc,
            inst_bits: None,
            rbid: None,
            iq_id: None,
            dq_id: None,
            ready_time_ps: None,
            has_disasm_annotation: false,
            disasm: format!("0x{:08x}", pc),
            tooltip: String::new(),
            stages: Vec::new(),
            current_stage: None,
            first_cycle: cycle,
            last_cycle: cycle,
            retire_status: RetireStatus::InFlight,
        }
    }

    fn close_current_stage(&mut self, end_cycle: u32) {
        if let Some((stage_idx, start)) = self.current_stage.take() {
            self.stages.push(StageSpan {
                stage_name_idx: stage_idx,
                lane: 0,
                _pad: 0,
                start_cycle: start,
                end_cycle,
            });
            if end_cycle > self.last_cycle {
                self.last_cycle = end_cycle;
            }
        }
    }

    fn open_stage(&mut self, stage_name_idx: u16, cycle: u32) {
        self.close_current_stage(cycle);
        self.current_stage = Some((stage_name_idx, cycle));
    }
}

/// Build an RV64GC instruction decoder from bundled ISA TOML definitions.
fn build_rv64gc_decoder() -> Option<Decoder> {
    Decoder::new(&[
        include_str!("../../isa/RV64I.toml").to_string(),
        include_str!("../../isa/RV64M.toml").to_string(),
        include_str!("../../isa/RV64A.toml").to_string(),
        include_str!("../../isa/RV32F.toml").to_string(),
        include_str!("../../isa/RV64D.toml").to_string(),
        include_str!("../../isa/RV64C.toml").to_string(),
        include_str!("../../isa/RV64C-lower.toml").to_string(),
        include_str!("../../isa/RV32_Zicsr.toml").to_string(),
        include_str!("../../isa/RV_Zifencei.toml").to_string(),
    ])
    .ok()
}

/// Decode a single instruction using the decoder. Returns mnemonic or hex fallback.
fn decode_instruction(decoder: &Decoder, inst_bits: u32) -> String {
    // Compressed instructions have the two LSBs != 0b11
    let bit_width = if inst_bits & 0x3 != 0x3 { 16 } else { 32 };
    decoder
        .decode_from_u32(inst_bits, bit_width)
        .unwrap_or_else(|_| format!("0x{:08x}", inst_bits))
}

/// Detect whether an annotation looks like a disassembly line by checking if
/// it starts with a hex address that matches the entity's known PC.
/// Handles formats like "00001000: jal zero, 0x10" and "0x80000000 addi x1, x0, 1".
fn is_disasm_line(text: &str, pc: u64) -> bool {
    let trimmed = text.trim();
    if let Some(first_word) = trimmed.split_whitespace().next() {
        let word = first_word.strip_suffix(':').unwrap_or(first_word);
        let hex_str = word.strip_prefix("0x").unwrap_or(word);
        if hex_str.len() >= 4 {
            if let Ok(addr) = u64::from_str_radix(hex_str, 16) {
                return pc != 0 && addr == pc;
            }
        }
    }
    false
}

fn populate_metadata(
    reader: &Reader,
    path: &Path,
    ids: &CpuProtocolIds,
    trace: &mut PipelineTrace,
) {
    let header = reader.header();
    let schema = reader.schema();

    // File info
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    trace.metadata.push(("File".into(), file_name));
    trace.metadata.push((
        "Format".into(),
        format!("µScope v{}.{}", header.version_major, header.version_minor),
    ));

    // Flags
    let mut flags = Vec::new();
    if header.flags & F_COMPRESSED != 0 {
        flags.push("compressed");
    }
    if header.flags & F_INTERLEAVED_DELTAS != 0 {
        flags.push("interleaved");
    }
    if header.flags & F_COMPACT_DELTAS != 0 && header.flags & F_INTERLEAVED_DELTAS == 0 {
        flags.push("compact-deltas");
    }
    if !flags.is_empty() {
        trace.metadata.push(("Flags".into(), flags.join(", ")));
    }

    // DUT properties (all of them)
    for (key, value) in reader.dut_properties() {
        trace.metadata.push((key.to_string(), value.to_string()));
    }

    // Clock
    if !schema.clock_domains.is_empty() {
        let cd = &schema.clock_domains[0];
        let freq_mhz = 1_000_000.0 / cd.period_ps as f64;
        let name = schema.get_string(cd.name).unwrap_or("?");
        trace.metadata.push((
            "Clock".into(),
            format!("{} ({} ps, {:.0} MHz)", name, cd.period_ps, freq_mhz),
        ));
    }

    // Pipeline stages
    trace
        .metadata
        .push(("Pipeline".into(), ids.stage_names.join(" → ")));

    // Trace stats
    let total_us = header.total_time_ps as f64 / 1e6;
    let total_cycles = header.total_time_ps / ids.period_ps;
    trace.metadata.push((
        "Duration".into(),
        format!("{} cycles ({:.1} µs)", total_cycles, total_us),
    ));
    trace
        .metadata
        .push(("Segments".into(), format!("{}", header.num_segments)));

    // Schema summary
    trace.metadata.push((
        "Schema".into(),
        format!(
            "{} storages, {} events, {} enums",
            schema.storages.len(),
            schema.events.len(),
            schema.enums.len(),
        ),
    ));

    // String table
    if let Some(st) = reader.string_table() {
        let mut count = 0u32;
        while st.get(count).is_some() {
            count += 1;
        }
        trace
            .metadata
            .push(("Strings".into(), format!("{} entries", count)));
    }
}

fn parse_uscope(path: &Path) -> Result<PipelineTrace, TraceError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| TraceError::UnsupportedFormat("invalid path encoding".into()))?;

    let mut reader = Reader::open(path_str).map_err(TraceError::Io)?;
    let ids = resolve_cpu_protocol(&reader)?;

    let mut trace = PipelineTrace::new();
    let decoder = build_rv64gc_decoder();

    // Populate trace metadata from the uscope file.
    populate_metadata(&reader, path, &ids, &mut trace);
    trace.period_ps = Some(ids.period_ps);

    // Pre-intern stage names
    let stage_name_indices: Vec<u16> = ids
        .stage_names
        .iter()
        .map(|name| trace.intern_stage(name))
        .collect();

    // Entity tracking state
    let mut slot_to_entity: HashMap<u16, u32> = HashMap::new();
    let mut builders: HashMap<u32, InstrBuilder> = HashMap::new();
    let mut finalized: Vec<InstrBuilder> = Vec::new();
    let mut next_reflex_id: u32 = 0;

    let num_segments = reader.segment_count();

    for seg_idx in 0..num_segments {
        let (_storages, items) = reader.segment_replay(seg_idx).map_err(TraceError::Io)?;

        for item in items {
            match item {
                TimedItem::Op(op) => {
                    if op.storage_id != ids.entities_storage_id {
                        continue;
                    }
                    let cycle = (op.time_ps / ids.period_ps) as u32;

                    match op.action {
                        DA_SLOT_SET => {
                            if op.field_index == ids.field_entity_id {
                                // New entity born
                                let entity_id = op.value as u32;
                                let reflex_id = next_reflex_id;
                                next_reflex_id += 1;

                                slot_to_entity.insert(op.slot, entity_id);
                                builders.insert(
                                    entity_id,
                                    InstrBuilder::new(entity_id, reflex_id, 0, cycle),
                                );
                            } else if op.field_index == ids.field_pc {
                                // Update PC on existing entity
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.pc = op.value;
                                        b.disasm = format!("0x{:08x}", op.value);
                                    }
                                }
                            } else if Some(op.field_index) == ids.field_inst_bits {
                                // Store raw instruction bits for decoding
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.inst_bits = Some(op.value as u32);
                                    }
                                }
                            } else if Some(op.field_index) == ids.field_rbid {
                                // Store retire buffer ID
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.rbid = Some(op.value as u32);
                                    }
                                }
                            } else if Some(op.field_index) == ids.field_iq_id {
                                // Store issue queue ID
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.iq_id = Some(op.value as u32);
                                    }
                                }
                            } else if Some(op.field_index) == ids.field_dq_id {
                                // Store dispatch queue ID
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.dq_id = Some(op.value as u32);
                                    }
                                }
                            } else if Some(op.field_index) == ids.field_ready_time_ps {
                                // Store ready time
                                if let Some(&eid) = slot_to_entity.get(&op.slot) {
                                    if let Some(b) = builders.get_mut(&eid) {
                                        b.ready_time_ps = Some(op.value);
                                    }
                                }
                            }
                        }
                        DA_SLOT_CLEAR => {
                            // Entity retired (cleared from catalog)
                            if let Some(entity_id) = slot_to_entity.remove(&op.slot) {
                                if let Some(mut b) = builders.remove(&entity_id) {
                                    b.close_current_stage(cycle);
                                    if b.retire_status == RetireStatus::InFlight {
                                        b.retire_status = RetireStatus::Retired;
                                    }
                                    finalized.push(b);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                TimedItem::Event(ev) => {
                    let cycle = (ev.time_ps / ids.period_ps) as u32;

                    if ev.event_type_id == ids.stage_transition_event_id {
                        if ev.payload.len() >= 5 {
                            let entity_id = u32::from_le_bytes(ev.payload[..4].try_into().unwrap());
                            let stage = ev.payload[4] as usize;

                            if let Some(b) = builders.get_mut(&entity_id) {
                                let stage_idx = stage_name_indices.get(stage).copied().unwrap_or(0);
                                b.open_stage(stage_idx, cycle);
                            }
                        }
                    } else if ev.event_type_id == ids.flush_event_id {
                        if ev.payload.len() >= 4 {
                            let entity_id = u32::from_le_bytes(ev.payload[..4].try_into().unwrap());

                            if let Some(mut b) = builders.remove(&entity_id) {
                                b.close_current_stage(cycle);
                                b.retire_status = RetireStatus::Flushed;
                                // Also remove from slot map
                                let slot = entity_id as u16;
                                slot_to_entity.remove(&slot);
                                finalized.push(b);
                            }
                        }
                    } else if ev.event_type_id == ids.annotate_event_id {
                        if ev.payload.len() >= 8 {
                            let entity_id = u32::from_le_bytes(ev.payload[..4].try_into().unwrap());
                            let text_ref = u32::from_le_bytes(ev.payload[4..8].try_into().unwrap());

                            if let Some(b) = builders.get_mut(&entity_id) {
                                if let Some(st) = reader.string_table() {
                                    if let Some(text) = st.get(text_ref) {
                                        // Detect disasm: annotation that starts
                                        // with a hex address matching the entity PC.
                                        if is_disasm_line(text, b.pc) {
                                            b.disasm = text.to_string();
                                            b.has_disasm_annotation = true;
                                        } else {
                                            if !b.tooltip.is_empty() {
                                                b.tooltip.push('\n');
                                            }
                                            b.tooltip.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                    } else if ev.event_type_id == ids.dependency_event_id && ev.payload.len() >= 9 {
                        let src_id = u32::from_le_bytes(ev.payload[..4].try_into().unwrap());
                        let dst_id = u32::from_le_bytes(ev.payload[4..8].try_into().unwrap());
                        let dep_type = ev.payload[8];

                        let kind = match dep_type {
                            0 => DepKind::Data,   // raw
                            1 => DepKind::Data,   // war
                            2 => DepKind::Data,   // waw
                            3 => DepKind::Memory, // structural
                            _ => DepKind::Data,
                        };

                        trace.dependencies.push(Dependency {
                            producer: src_id,
                            consumer: dst_id,
                            kind,
                        });
                    }
                }
            }
        }
    }

    // Finalize remaining in-flight instructions
    for (_eid, b) in builders.drain() {
        let mut b = b;
        b.close_current_stage(b.last_cycle.saturating_add(1));
        finalized.push(b);
    }

    // Sort by first_cycle, then by reflex_id for stable ordering
    finalized.sort_by(|a, b| {
        a.first_cycle
            .cmp(&b.first_cycle)
            .then(a.reflex_id.cmp(&b.reflex_id))
    });

    // Emit to PipelineTrace
    for mut b in finalized {
        // Decode instruction bits into mnemonic if no annotation already provided disasm
        if !b.has_disasm_annotation {
            if let (Some(bits), Some(dec)) = (b.inst_bits, &decoder) {
                let mnemonic = decode_instruction(dec, bits);
                if b.pc != 0 {
                    b.disasm = format!("0x{:08x} {}", b.pc, mnemonic);
                } else {
                    b.disasm = mnemonic;
                }
            }
        }

        let stage_start = trace.stages.len() as u32;
        trace.stages.extend(b.stages);
        let stage_end = trace.stages.len() as u32;

        trace.add_instruction(InstructionData {
            id: b.entity_id,
            sim_id: b.entity_id as u64,
            thread_id: 0,
            rbid: b.rbid,
            iq_id: b.iq_id,
            dq_id: b.dq_id,
            ready_cycle: b.ready_time_ps.map(|t| (t / ids.period_ps) as u32),
            disasm: b.disasm,
            tooltip: b.tooltip,
            stage_range: stage_start..stage_end,
            retire_status: b.retire_status,
            first_cycle: b.first_cycle,
            last_cycle: b.last_cycle,
        });
    }

    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_uscope_magic() {
        let source = UscopeSource;
        assert!(source.detect(&MAGIC));
        assert!(source.detect(&[0x75, 0x53, 0x43, 0x50, 0x00, 0x01]));
        assert!(!source.detect(&[0x00, 0x00, 0x00, 0x00]));
        assert!(!source.detect(&[0x75, 0x53])); // too short
    }

    #[test]
    fn load_stream_returns_error() {
        let source = UscopeSource;
        let mut data: &[u8] = &[];
        assert!(source.load(&mut data).is_err());
    }

    #[test]
    fn test_is_disasm_line() {
        assert!(is_disasm_line("00001000: jal zero, 0x10", 0x00001000));
        assert!(is_disasm_line("0x80000000 addi x1, x0, 1", 0x80000000));
        assert!(!is_disasm_line("(g:4,c0)\\n", 0x00001000));
        assert!(!is_disasm_line("i-cache-miss\\n", 0x00001000));
        assert!(!is_disasm_line("00001000: jal zero, 0x10", 0)); // pc=0 never matches
    }

    #[test]
    fn load_sample_uscope() {
        let path = std::path::Path::new("test_data/sample.uscope");
        if !path.exists() {
            return; // skip if test data not available
        }
        let source = UscopeSource;
        let trace = source.load_file(path).unwrap();
        assert!(trace.row_count() > 0, "should have instructions");

        // Instruction 0 should have disasm with mnemonic, not just hex address
        let instr0 = &trace.instructions[0];
        eprintln!("instr0 disasm: {:?}", instr0.disasm);
        eprintln!("instr0 tooltip: {:?}", instr0.tooltip);
        // First instruction that has disasm should contain a mnemonic
        let has_mnemonic = trace.instructions.iter().any(|i| {
            i.disasm.contains("jal") || i.disasm.contains("addi") || i.disasm.contains("auipc")
        });
        assert!(
            has_mnemonic,
            "at least some instructions should have mnemonics in disasm"
        );
    }
}
