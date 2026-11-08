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
The [module-set comparison](../../../docs/windows11-commonality.md) and
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

The 5.14.1 default registry contains five profiles, adding
`windows11-26200.8037-x64.json` and `windows11-26100.1742-x64.json` after both
passed isolated single-monitor lab acceptance for exact modules/layout, clock
updates/restoration and tray preparation/Stop. These exact module sets come
from a Windows 11 Pro 25H2 8037 image and the official Enterprise LTSC 2024
Evaluation 1742 image. Default and explicit `compatibility-lab` builds now
contain the same five profiles; the feature remains reserved for future
candidates. The earlier default-three/lab-five artifacts remain historical
records. The same 5.14.1 default binaries passed the complete six-check scoped
reruns in both awakened guests, including execution-state restoration and
normal parent-observed completion. The LTSC rerun used a verified Medium
parent and hidden Medium child on the same user/session/desktop after reboot;
the test package and application command deadlines were unchanged.

Four other scenarios remain unvalidated. LTSC's first inspection response
failed and a later run of the same package passed; the initial cause remains
unresolved. The 8037 parent wait timed out before its worker finished; the
original worker's late complete results passed independent review, while the
parent timeout remains recorded. Screen painting and independent native
refresh were not validated by either lab run. The default LTSC rerun adds
five independently inspected screenshots of all three written texts, native
restoration and a later advancing clock; these sampled images do not establish
continuous native refresh or callback counts. Its evaluation-license scheduled
shutdown and first default-launch precondition failure are recorded separately
from the earlier unresolved taskbar response failure. Their provenance,
changed fields, analyzer rejection tests and runtime evidence are recorded in
[the next-version evidence](../../../docs/compatibility-next-versions.md).

The 8457 validation record remains bound to its own tested lab and default
release binaries; it does not validate the new candidates. Offline candidate
artifacts are not automatically registered or activated, and neither runtime
settings nor external JSON can override the compiled module identities.
