# Built-in taskbar compatibility evidence

The 5.16.2 candidate contains nine default profiles, including the primary-clock-only
21H2 implementation and the 26H1 28000.1 implementation. Its final default-package
rerun is still pending. The explicit `compatibility-lab` registry adds only the
28000.3365 candidate, for ten profiles in total. Version 5.16.0 had eight default
profiles; version 5.14.2 had seven. `windows11-22621.1702-x64.json` (22H2 Pro) and
`windows11-22631.2861-x64.json` (23H2 Pro) were promoted after independently
installed guests passed six scoped checks and independent evidence review.
Both use the separately implemented four-argument borrowed-element
`system_tray_drag_drop_borrowed_coordinates_x64_v1` ABI. In 5.15.0 LTSC also uses this borrowed ABI after caller analysis found its previous owned entry only served Copilot. The remaining four retain the owned-element ABI, with consumer purpose explicitly recorded in `tray_geometry`. Exact modules/layout, primary-clock
updates/restoration and tray preparation/stop have recorded acceptance.
Those older single-monitor runs do not establish input, replication,
multiple-monitor or mixed-DPI acceptance.
See [22H2 / 23H2 evidence](../../../docs/compatibility-22h2-23h2.md) for package
identities, raw restoration checks and the separate default reruns.

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
`compatibility::profiles()` validates the registry once. Since 5.15.0, each module is recognized independently by exact identity/hash or the strict execution-equivalence fingerprint. Host/source fields remain owned by their respective modules; compatible interface contracts may compose. The injected adapter independently repeats recognition and checks actual loaded identities and object ownership. Exact-only historical acceptance is never inherited by an equivalent image or a composed set. See [module recognition](../../../docs/module-recognition.md). The production path does not
accept external JSON overrides. Recipe and ABI enums name Rust implementations;
data cannot add a new calling convention or arbitrary memory operation.

Disk matching is a prerequisite, not proof of an active or compatible live
session. Loaded-image identity, code, ownership, thread and XamlRoot checks
remain mandatory. Known notification/menu limitations remain documented.

Add another profile only with its exact module and symbol evidence, a reviewed
implementation of any new ABI, and explicit validation results. Do not widen
an OS build range or reuse offsets merely because version labels look similar.
The profile contains metadata and minimal code prefixes, not system DLLs/PDBs.

The historical 5.14.1 default registry contains five profiles, adding
`windows11-26200.8037-x64.json` and `windows11-26100.1742-x64.json` after both
passed isolated single-monitor lab acceptance for exact modules/layout, clock
updates/restoration and tray preparation/Stop. These exact module sets come
from a Windows 11 Pro 25H2 8037 image and the official Enterprise LTSC 2024
Evaluation 1742 image. Default and explicit `compatibility-lab` builds at that
revision contain the same five profiles; the feature remains reserved for future
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

The default registry includes `windows11-22000.318-x64-clock.json` after an
independently installed guest passed primary-clock text updates, width changes
and Stop restoration with the frozen laboratory package. The final default
package has its own verification record. This early host has five real roles,
no FrameHeight field or factory vtable, and thirteen individually verified
child-vtable entries. It uses a separately verified NotificationAreaHost route.
Its tray module, ABI, purpose and entries are all absent; incomplete tray
evidence is rejected. Only the selected primary clock is supported. See
[the exact 21H2 scope and package identities](../../../docs/compatibility-21h2.md).

The lab feature also includes `lab/windows11-28000.3365-x64-notification.json`.
It selects the independently reviewed notification-area owned ABI and keeps
unreviewed drag/drop excluded. The current experiment requires exact image
identities; it has no implementation-equivalence fingerprint or runtime
acceptance record. A frozen lab EXE matches the retained Microsoft module
bytes in a disk-only report. Installed paths, a coherent installed system,
loaded modules and actual clock/tray behavior remain unverified. See
[26H1 research and integration](../../../docs/windows11-coverage-next-research.md).

`windows11-28000.1-x64-notification.json` separately describes the canonical
26H1 base system, whose notification code resides in `Taskbar.View.dll`. Its
exact module bytes, matching symbols, actual ec97 child interface, source fields
and owned notification-area ABI have been reviewed. The actual installed and
Explorer-loaded paths, full hashes and image sizes match those files. A frozen
lab package on the independently installed Pro x64 single-display guest completed
clock text/width updates and restoration, tray preparation/Stop, four primary-clock
calendar clicks and 75 seconds of native-clock observation after normal exit,
with the same Explorer process. These are the lab package's results; the final
default 5.16.2 package still requires its own rerun.

The base profile has its own addresses and three reviewed execution fingerprints;
it does not reuse the later SystemTray profile or permit new implementation bytes.
The default registry has 26 module records, 23 with non-null fingerprints. The lab
registry adds three exact-only 3365 records. Retain all live object, thread, window
and XamlRoot checks; matching metadata alone does not inherit acceptance. ARM64,
26H1 multiple monitors, mixed DPI, tray replication and the complete pointer-input
matrix remain outside these results. See [26H1 x64 evidence](../../../docs/compatibility-26h1.md).

The 21H2 implementation carries a separately verified NotificationAreaHost
route and recognizes the observed single DateTimeInnerTextBlock clock. Its
combined lines share the first line's font, color and alignment. The adjacent
notification bell is excluded. Source discovery, isolated update/restore and
real taskbar acceptance are distinct evidence levels in the 21H2 record.
