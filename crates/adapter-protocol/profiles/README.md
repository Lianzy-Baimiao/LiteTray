# Built-in taskbar compatibility evidence

`windows11-26300.9711-x64.json` consolidates the three previously pinned system
images, source offsets, concrete vtables, and tray coordinate entry points.
The six host entries were resolved from the matching Microsoft PDB cache and
their section bounds and prologues checked against the exact taskbar image.
Provenance: [source binding](../../../docs/taskbar-source-binding.md),
[adapter ABI](../../taskbar-adapter/ABI.md) and
[live acceptance](../../../docs/live-clock-acceptance.md).

`windows11-26300.9818-x64.json` adds the exact updated module set. Its symbol,
layout and validation evidence is recorded in
[the 9818 compatibility record](../../../docs/compatibility-9818.md).

The JSON files are compiled into both host and adapter with `include_bytes!`.
`compatibility::profiles()` validates the registry once. Selection uses the
complete taskbar image identity and additionally requires its disk SHA-256;
the selected profile supplies all required shell and tray evidence. Unknown
or ambiguous taskbar identities do not fall back to another profile, and
modules from different profiles cannot be combined. The production path does not
accept external JSON overrides. Recipe and ABI enums name Rust implementations;
data cannot add a new calling convention or arbitrary memory operation.

Disk matching is a prerequisite, not proof of an active or compatible live
session. Loaded-image identity, code, ownership, thread and XamlRoot checks
remain mandatory. Known notification/menu limitations remain documented.

Add another profile only with its exact module and symbol evidence, a reviewed
implementation of any new ABI, and explicit validation results. Do not widen
an OS build range or reuse offsets merely because version labels look similar.
The profile contains metadata and minimal code prefixes, not system DLLs/PDBs.

The candidate in `lab/windows11-26200.8457-x64.json` is excluded from normal
builds. Disposable VM builds may opt in with the `compatibility-lab` Cargo
feature on both the host and taskbar adapter. Its `Taskbar.View.dll` identity,
field and ABI evidence is recorded in
[the three-set comparison](../../../docs/windows11-commonality.md).
Its pending runtime validation must not be described as production support.
The feature cannot be enabled by a runtime setting, environment variable or
external JSON. Default packages must be built without this feature.
