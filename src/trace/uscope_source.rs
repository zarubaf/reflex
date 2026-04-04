//! Thin adapter between `uscope_cpu::CpuTrace` and Reflex's internal types.
//!
//! Provides `open_uscope()` which wraps `CpuTrace::open()` to produce a
//! `PipelineTrace` (Reflex's GUI view model) plus a `CpuTrace` for lazy
//! loading and buffer/counter queries.

use crate::trace::model::PipelineTrace;
use crate::trace::TraceError;
use std::path::Path;
use uscope_cpu::CpuTrace;

/// Open a uscope file, returning a metadata-only PipelineTrace and a CpuTrace
/// for on-demand segment loading and buffer/counter queries.
///
/// The PipelineTrace has counter names, buffers, stage names, and metadata
/// populated but empty instructions/stages/dependencies.
pub fn open_uscope(path: &Path) -> Result<(PipelineTrace, CpuTrace), TraceError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| TraceError::UnsupportedFormat("invalid path encoding".into()))?;

    let cpu_trace = CpuTrace::open(path_str).map_err(TraceError::Io)?;

    let mut trace = PipelineTrace::new();

    // Populate metadata from CpuTrace.
    trace.metadata = cpu_trace.metadata().to_vec();
    trace.period_ps = Some(cpu_trace.period_ps());

    // Intern stage names.
    for name in cpu_trace.stage_names() {
        trace.intern_stage(name);
    }

    // Counters: copy counter series from CpuTrace.
    trace.counters = cpu_trace.counter_series().to_vec();

    // Buffers: copy buffer infos from CpuTrace.
    trace.buffers = cpu_trace.buffer_infos().to_vec();

    // Set total instruction count from TraceSummary if available.
    if let Some(summary) = cpu_trace.trace_summary() {
        trace.total_instruction_count = summary.total_instructions as usize;
    } else {
        eprintln!("Warning: no TraceSummary in file; instruction count unknown");
        trace.total_instruction_count = 0;
    }

    // Set max_cycle from CpuTrace (covers all segments).
    trace.max_cycle_override = Some(cpu_trace.max_cycle());

    Ok((trace, cpu_trace))
}
