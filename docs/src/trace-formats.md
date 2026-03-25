# Trace Formats

Reflex supports two trace formats. The format is auto-detected from the file's magic bytes.

## µScope (`.uscope`)

Binary format from [µScope](https://github.com/zarubaf/uscope) with CPU protocol semantics.

**Features:**
- LZ4 compression
- Checkpointed random access via segments
- Performance counters (automatically detected)
- Queue metadata (retire/dispatch/issue queue configuration)
- Pipeline stage names from schema
- Instruction annotation via string table
- RISC-V instruction decoding (RV64GC)
- Clock domain information

**DUT Properties used by Reflex:**

| Property | Purpose |
|----------|---------|
| `cpu.pipeline_stages` | Pipeline stage names (comma-separated) |
| `cpu.retire_queue_size` | Retire buffer slot count |
| `cpu.dispatch_queue_stages` | Stage names for dispatch queue membership |
| `cpu.dispatch_queue_names` | Display names for dispatch queues |
| `cpu.issue_queue_stages` | Stage names for issue queue membership |
| `cpu.issue_queue_names` | Display names for issue queues |
| `cpu.retire_queue_stages` | Stage names for retire queue membership |

**Trace metadata** shown in the info overlay (Cmd+I):
- File name, format version, flags
- Clock domain (name, period, frequency)
- Pipeline stage sequence
- Duration (cycles and microseconds)
- Segment count, schema summary, string table size

## Konata (`.log`, `.konata`, `.kanata`)

Text-based [Kanata log format](https://github.com/shioyadan/Konata/blob/master/docs/kanata-log-format.md) used by CPU simulators. Compatible with [Konata](https://github.com/shioyadan/Konata).

**Features:**
- Human-readable text format
- Stage transitions, labels, dependencies
- Flush/squash events

**Limitations compared to µScope:**
- No performance counters
- No queue metadata
- No compression or random access
- No instruction decoding (uses labels from the trace)

## Generating Test Traces

Press **Cmd+G** to generate a random synthetic trace for testing. The generator creates a configurable number of instructions with realistic pipeline stage patterns, dependencies, and timing.
