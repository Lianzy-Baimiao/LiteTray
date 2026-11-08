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

`windows11-26200.8457-x64.json` adds the exact Windows 11 Pro 25H2 module set,
whose tray code resides in Core/Taskbar.View.dll. Matching symbols and instructions
establish the shared source fields and tray ABI; an independently installed VM
passed real primary-clock text/width updates, restoration and tray preparation/stop.
The [three-set comparison](../../../docs/windows11-commonality.md) and
[25H2 runtime evidence](../../../docs/compatibility-8457.md) record the exact scope,
failed attempts and tested binary hashes. Input, tray replication, multiple-monitor
and mixed-DPI acceptance remain pending.

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

The default registry contains the three reviewed profiles above. The explicit
`compatibility-lab` Cargo feature additionally includes
`lab/windows11-26200.8037-x64.json` and `lab/windows11-26100.1742-x64.json`, for a
five-profile lab registry. These exact module sets come from a Windows 11 Pro
25H2 8037 image and the official Enterprise LTSC 2024 Evaluation 1742 image.
The LTSC candidate passed isolated single-monitor lab acceptance for exact
modules/layout, clock updates/restoration and tray preparation/Stop. Four other
scenarios remain unvalidated. Its first inspection response failed and a later
run of the same package passed; the initial cause remains unresolved. The 8037
candidate still awaits live acceptance, and default builds exclude both.
Their provenance, changed fields, analyzer rejection tests and runtime
evidence are recorded in
[the next-version evidence](../../../docs/compatibility-next-versions.md).

The 8457 validation record remains bound to its own tested lab and default
release binaries; it does not validate the new candidates. Offline candidate
artifacts are not automatically registered or activated, and neither runtime
settings nor external JSON can override the compiled module identities.
