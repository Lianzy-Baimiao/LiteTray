# Offline compatibility candidate inspection

This developer tool reads an extracted Windows file tree or a running system's files, acquires exact Microsoft public PDBs, and checks bounded x64 instruction recipes. It produces **candidate evidence**, never a production profile or an activation command. It does not load the inspected DLLs, open Explorer, modify Windows, edit the production registry, or run private APIs.

Use 64-bit Python on Windows. Install the pinned Python dependencies into a development environment:

```powershell
python -m pip install -r tools/compatibility/requirements.txt
```

For the extracted Windows 11 25H2 image:

```powershell
python tools/compatibility/inspect_modules.py `
  --system-root local/windows11-25h2/Windows `
  --output local/compat-26200.8457-new-evidence `
  --label "Windows 11 Pro 25H2 26200.8457, image index 3"
```

`--system-root` is the directory containing `System32` and `SystemApps`. `--output` must be new. The optional label is for inventory and is never used to select offsets. Known filenames are discovered only when exactly one candidate exists for a role. Missing or ambiguous roles are rejected; specify the real relative path explicitly when needed:

```powershell
python tools/compatibility/inspect_modules.py `
  --system-root C:/Windows `
  --module tray=SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll `
  --output local/compat-current-new-evidence `
  --symbol-cache local/compat-26300.9818/symbols `
  --offline
```

Repeat `--module ROLE=relative/path.dll` for `taskbar`, `shell`, and `tray` if their file locations differ. The reported module keeps its actual filename: `Taskbar.View.dll` is never relabeled `SystemTray.dll`. Production support requires an exact built-in profile matching the complete module set and independent live evidence for the claimed scenarios. A filename alone cannot establish or rule out that match. This tool does not check the production registry; recognizing a reviewed module variant in the parser does not add it to that registry.

By default, exact PDBs are downloaded into `OUTPUT/symbols/`. `--symbol-cache` reuses a cache; `--offline` forbids new downloads. Existing wrong-identity PDBs are rejected and preserved. Downloads become cache entries only after GUID and DBI-age checks. DbgHelp enumeration is serial, uses a separate noninvading session token, and verifies the image/PDB identity with `SYMOPT_EXACT_SYMBOLS` and `SymGetModuleInfo64`.

Outputs:

- `candidate.json`: independent `litetray.compatibility.candidate.v1` schema, exact identities, module roles, derived offsets, symbol provenance, analysis blockers, and pending live validation. Tray geometry includes per-consumer traits and their own review status. `production_ready` is always `false`; the production `Profile` decoder cannot ingest this envelope.
- `taskbar-symbols.json`, `shell-symbols.json`, `tray-symbols.json`: exact symbol mappings and PDB verification.
- `disassembly.json` and `disassembly.txt`: decoded instructions used by the reviewed recipe checks.
- PDB cache: local analysis input only; do not commit Microsoft PDBs or Windows DLLs.

Exit `0` means the required module and common-layout checks completed and the available geometry traits were classified. An optional geometry trait can still be `unreviewed`; read `candidate_layouts.tray.screen_rect_traits[*].analysis_blockers` and `geometry_capabilities` before consuming it. Exit `0` never means the OS or every capability is supported. Exit `2` means missing required evidence, ambiguous modules/symbol identities, invalid function boundaries, unsupported architecture, or other rejected input; inspect top-level `analysis_blockers`. File strings and symbol names are topology clues only: they cannot prove the real tree or its thread/window/XamlRoot ownership.

The implementation recognizes the bounded `TaskbarHost`/`IXamlExplorerHostChildWindow` field chain and two common tray coordinate members in the reviewed x64 images. Screen-rectangle functions have separate owned-value and borrowed-value ABI recipes; their filename and Windows marketing version do not select an ABI. Both signatures can legitimately coexist at distinct nonoverlapping functions. Every signature must resolve to one exact RVA; duplicate bindings, shared addresses and overlapping function bounds are rejected. New instruction shapes must be inspected and added as explicit recipes; do not relax a check merely to obtain a candidate. Promotion requires code review, an implemented module/ABI variant where necessary, and feature-specific VM live results. This tool deliberately has no promotion or `--force` option.

`screen_rect_traits` records each observed signature's `entry`, `abi_candidate`, `abi_status`, ownership evidence, consumer `usage`, overall `status`, and scoped `analysis_blockers`. The auditable trait IDs are `copilot_button_owned`, `notification_area_owned`, and `drag_drop_borrowed`; an owned function whose consumer proof fails remains `owned_screen_rect` / `unreviewed`. An instruction or ownership failure disables that trait without hiding evidence for an independent trait. A consumer must require both `status: reviewed` and its specific purpose in `geometry_capabilities`. `abi_status: reviewed` alone does not establish the function's use.

With two signatures, `implementation_selection` is `requires_consumer_choice`, top-level tray `abi_candidate` is `null`, and `entries` contains only the common coordinate members. The individual screen entries remain under their traits. There is no owned-first, borrowed-first, or first-success fallback. A single reviewed implementation retains the previous three-entry layout for that implementation. These fields describe static choices only; they do not select a production hook or inherit another module's live results.

Owned consumer evidence scans all aligned direct calls within unique `.pdata` bounds and accepts only exact reviewed caller signatures. Each caller must create a FrameworkElement wrapper through the actual IFrameworkElement IID, vtable slot 0 and exact CFG dispatch, transfer that local into RDX, pass a separate result buffer in RCX and HWND in R8, and read the returned RECT. Null-or-QI branches, local transfers and incoming branches into the checked window are verified. A mixed-purpose or unknown caller set is unreviewed. This audit does not prove that indirect callers are absent, which is explicit in its output.

The ordinary 24H2 BASE sample's owned function is called only by `CopilotIcon::ToggleEdgeCopilot`. The LTSC 26100.1742 sample adds `CopilotIcon::OnPointerEntered` and `OnPointerExited`; all three are still Copilot consumers. Their owned functions are **not evidence of notification-icon or drag/drop coverage**. Both samples independently pass the complete borrowed drag/drop recipe and four-caller proof. In the observed 26200.8037/8457 samples, owned consumers are `TryGetChevronScreenRect` and `TryGetIconScreenRect`; in 26300.9818 they are `OnScreenBoundsRequested` and `TryGetChevronScreenRect`. Those classify as `notification_area_owned`. Their separate 115/105-instruction borrowed functions remain unreviewed until complete recipes are added. This classification depends on decoded calls and object flow, not these build labels. The historical 26300.9711 symbol inventory without its original PE bytes cannot establish a current consumer classification. Preparation/stop evidence alone never establishes pointer, drag/drop or notification-boundary behavior.

The screen-rectangle function is checked for reviewed owned-value cleanup paths: an inline null check followed by `IUnknown::unconditional_release_ref`, or a call to the exact `IUnknown` destructor that performs that check. The form with a separate transform wrapper also requires a complete 106-instruction template: exact TransformToVisual/TransformBounds callees, actual GetDpiForWindow, MulDiv and MapWindowPoints import slots, arithmetic, result, branches and cleanup. Its exact stack address, initialization and adjacent cleanup sequence are checked; extra destructor calls cannot be ignored. Every direct call to the exact release leaf is counted in this form: only the single, null-guarded `[rbp-0x60]` input-temporary cleanup immediately after `TransformToVisual` is accepted. An early direct release of the incoming wrapper, an additional release, or a changed guard is rejected. Older owned argument/result/cleanup proofs are retained with `complete_instruction_template: false`; they must not be mistaken for a complete instruction equivalence proof. The release leaf has no unwind record, so it has a separate six-instruction recipe: save and clear the wrapper, select vtable slot 2 (`Release`), and tail-call the exact CFG dispatch pointer. Other functions require `.pdata` boundaries. Missing boundaries do not trigger a generic disassembly fallback.

The source getter has two reviewed register allocations. The newer form keeps the source field in RDI and calls the exact DesktopWindowXamlSource `Content` consumer without overwriting that field pointer. The 24H2 form keeps it in RSI and inlines the Content call through slot 6. Both require the exact ABI getter, null-preserving this adjustment, unique field and direct object flow; the RSI form additionally checks the field dereference, output location, vtable slot and bounded failure branch. These recipes derive the actual offsets: 24H2/LTSC 2024 has element `+0x40`, host-child `+0x18`, child-source `+0x10`; the reviewed 25H2 images have `+0x10`, `+0x38`, `+0x10`. A matching recipe is still only static candidate evidence.

Some exact public PDBs report three XamlExplorerHost functions as undecorated raw names. Only the explicitly listed raw names are accepted, with independent anchors: child vtable slot 8 for the ABI getter, a unique direct call from that getter for its implementation, and factory vtable slot 6 for the creator. The raw implementation name can name both a setter and a getter; the direct-call anchor resolves that overload. The raw creator additionally needs the reviewed argument sequence reaching the exact factory implementation. Conflicting decorated bindings are rejected. `Image.exact()` remains exact and unique, and the tool never invents decorated symbols or performs a global fuzzy-name fallback. Candidate `source.evidence.symbol_bindings` retains the actual raw names and each anchor.

The separate `system_tray_drag_drop_borrowed_coordinates_x64_v1` recipe accepts the exact `DragDropManager::ScreenRectForElement` member signature. Its parameters are RCX opaque manager, RDX hidden RECT result, R8 borrowed FrameworkElement wrapper and R9 HWND; RAX returns the original result pointer. RCX is overwritten before use. The complete bounded body must uniquely match one of three reviewed instruction templates: 114 instructions with inline TransformBounds and local null-guarded releases; 96 instructions with the exact TransformBounds consumer and local IUnknown destructor calls; or a distinct 114-instruction inline form with a tail HRESULT failure branch, different fixed stack locals and the exact DisplayInformation GetForCurrentView helper. Instruction count never selects a recipe on its own. Every call, branch, register, result store and stack cleanup in each body is checked. Named call targets are resolved independently, and MapWindowPoints must be the actual USER32 import or delay-import slot. The borrowed input is never passed to a destructor or release.

All four direct callers must be aligned decoded calls inside unique `.pdata` bounds and belong to the three exact reviewed DragDropManager methods. Unresolved byte hits, extra callers and changed call multiplicities are rejected. Each caller must release its same wrapper after the call. One caller uses a preserved zero register for its null check; its zero initialization and absence of intervening writes are verified. The delegated form also has callers that borrow RAX returned by an exact `as<FrameworkElement>` helper. Its complete 30-instruction body must return the original hidden wrapper buffer, load the actual IFrameworkElement IID, and call the exact IconView root `QueryInterface`. That callee has a separate complete 27-instruction template: it preserves the IID/output arguments, calls the exact inner `query_interface`, and uses vtable slot 0 through the exact CFG dispatch pointer when forwarding. An arbitrary bounded helper or IUnknown destructor cannot substitute for QueryInterface. The caller then invokes the checked IUnknown destructor on the same local wrapper.

The borrowed recipe also checks the four exact named cleanup symbols and their three distinct 12-byte parent-stack-local destructor leaves. These corroborate local cleanup; the tool does not parse the complete C++ exception metadata and reports `cpp_exception_metadata_parsed: false`. Other than the specifically documented six-instruction Release, five-byte CFG and twelve-byte cleanup leaves, functions still require exact `.pdata` boundaries.

The official Pro installation-media modules for 22H2 22621.1702 and the 23H2-labelled 22631.2861 ESD have both passed the static recipes. Both actual module sets retain the existing FrameHeight host witness, so no Show/controller substitute is introduced. Early discovery BASE files were different binaries and must not be used as proof of those installation-media sets. The 23H2 ESD XML reports the shared core 22621.2861; an installed 22631 build claim requires the guest's OS inventory. See [the 22H2/23H2 record](../../docs/compatibility-22h2-23h2.md) for the separate runtime validation status.

The input root is a collection boundary, not proof that files came from one installed OS. `coherent_installation_verified` remains `false`, including when every static recipe passes. Distinct modules may legitimately have different component version numbers. A known production profile and isolated loaded-image checks must validate the complete set; never pick one module's offsets from another candidate. The tool records actual filename casing; compare Windows paths case-insensitively, without treating different basenames as equivalent.

The independently reviewed 26200.8457 exact profile is included in the default registry; see [the three-set comparison](../../docs/windows11-commonality.md) and [its VM validation record](../../docs/compatibility-8457.md). Recorded acceptance covers the module/layout chain, primary clock update and stop/restore, and tray preparation and stop. Pointer interactions, tray/model mirroring, multiple monitors, and mixed DPI remain separate validation scopes. This static candidate output does not inherit those live results or become a production profile.

Run the rejection tests without system access or downloads:

```powershell
python -m unittest discover -s tools/compatibility -p test_evidence.py -v
```

For a regression against a known image, run with its exact cached PDBs and compare `candidate_layouts` to the reviewed profile. Keep that local result separate from live acceptance evidence. Source images must be obtained and verified independently; a PDB match identifies a binary but is not proof of installation-media provenance.

The offline tests include a coexisting owned/borrowed module with real caller windows, capability-specific degradation, duplicate/overlapping functions, wrong consumer/call target, changed owned arithmetic/imports, and the borrowed tail-error/cleanup path. They also cover argument-register swaps, wrong result/return pointers, wrong IAT or named callee, nonlocal/extra cleanup, changed null branches, caller wrapper/result aliasing, a conversion calling a destructor in place of QueryInterface, wrong QI slot/CFG/inner target, conflicting source aliases, and an E8 byte embedded inside an instruction operand. They use literal instruction fixtures and synthetic PE metadata, with no Windows DLL or PDB required.
