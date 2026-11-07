# Built-in taskbar compatibility evidence

`windows11-26300.9711-x64.json` consolidates the three previously pinned system
images, source offsets, concrete vtables, and tray coordinate entry points.
The six host entries were resolved from the matching Microsoft PDB cache and
their section bounds and prologues checked against the exact taskbar image.
Provenance: [source binding](../../../docs/taskbar-source-binding.md),
[adapter ABI](../../taskbar-adapter/ABI.md) and
[live acceptance](../../../docs/live-clock-acceptance.md).

The JSON is compiled into both host and adapter with `include_bytes!`.
`compatibility::builtin()` validates it once. The production path does not
accept external JSON overrides. Recipe and ABI enums name Rust implementations;
data cannot add a new calling convention or arbitrary memory operation.

Disk matching is a prerequisite, not proof of an active or compatible live
session. Loaded-image identity, code, ownership, thread and XamlRoot checks
remain mandatory. Known notification/menu limitations remain documented.

Add another profile only with its exact module and symbol evidence, a reviewed
implementation of any new ABI, and explicit validation results. Do not widen
an OS build range or reuse offsets merely because version labels look similar.
The profile contains metadata and minimal code prefixes, not system DLLs/PDBs.
