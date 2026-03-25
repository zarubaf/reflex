# Queue Panels

The queue panels show the state of the CPU's internal queues at the current cursor cycle. They update in real-time as you move the cursor.

## Queue Types

### Retire Queue

Shows the retire buffer (ROB) occupancy. Each slot shows:
- Slot index (hex)
- Current pipeline stage
- Instruction disassembly

The header shows `Retire Queue (occupied/total) @ cycle N`.

### Dispatch Queues

Shows dispatch queue entries grouped by queue instance. Each entry shows:
- Instruction disassembly
- Wait time in cycles since entering the dispatch stage

Queue names come from the µScope DUT property `cpu.dispatch_queue_names`.

### Issue Queues

Shows issue queue entries with ready/waiting status:
- Green dot = ready to issue (all operands available)
- Red dot = waiting (operands pending)

Each entry shows instruction disassembly and wait time. The header shows ready/total counts.

## Data Source

Queue data is NOT pre-computed — it's calculated on-the-fly from the pipeline trace at the cursor cycle. The queue panel reads metadata from the µScope trace:

| DUT Property | Purpose |
|-------------|---------|
| `cpu.retire_queue_size` | Number of ROB slots (default: 128) |
| `cpu.dispatch_queue_stages` | Stage names that represent "in dispatch queue" |
| `cpu.dispatch_queue_names` | Display names for dispatch queue instances |
| `cpu.issue_queue_stages` | Stage names that represent "in issue queue" |
| `cpu.issue_queue_names` | Display names for issue queue instances |
| `cpu.retire_queue_stages` | Stage names that represent "in retire queue" |

## Layout

Queue panels are docked in a configurable position:

| Key | Layout |
|-----|--------|
| Alt+Cmd+1 | Bottom (horizontal split) |
| Alt+Cmd+2 | Left (vertical split) |
| Alt+Cmd+3 | Right (vertical split) |

Toggle queue panel visibility with **Cmd+B**.

The Issue Queue panel shares a tab bar with the Log panel.
