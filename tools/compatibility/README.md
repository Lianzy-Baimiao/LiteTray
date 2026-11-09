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

- `candidate.json`: independent `litetray.compatibility.candidate.v1` schema, exact identities, module roles, derived offsets, symbol provenance, analysis blockers, and pending live validation. `production_ready` is always `false`; the production `Profile` decoder cannot ingest this envelope.
- `taskbar-symbols.json`, `shell-symbols.json`, `tray-symbols.json`: exact symbol mappings and PDB verification.
- `disassembly.json` and `disassembly.txt`: decoded instructions used by the reviewed recipe checks.
- PDB cache: local analysis input only; do not commit Microsoft PDBs or Windows DLLs.

Exit `0` means all implemented **static** recipes passed. It never means the OS is supported. Exit `2` means missing evidence, ambiguous modules/symbols, unsupported instructions/architecture, or other rejected input; inspect `analysis_blockers`. A partially analyzed candidate remains blocked. File strings and symbol names are topology clues only: they cannot prove the real tree or its thread/window/XamlRoot ownership.

The implementation recognizes the bounded `TaskbarHost`/`IXamlExplorerHostChildWindow` field chain and three required tray coordinate entries in the reviewed x64 images. The screen-rectangle entry has separate owned-value and borrowed-value ABI recipes; its filename and Windows marketing version do not select an ABI. A missing or ambiguous entry stays blocked. New instruction shapes must be inspected and added as explicit recipes; do not relax a check merely to obtain a candidate. Promotion requires code review, an implemented module/ABI variant where necessary, and feature-specific VM live results. This tool deliberately has no promotion or `--force` option.

The screen-rectangle function is checked for reviewed owned-value cleanup paths: an inline null check followed by `IUnknown::unconditional_release_ref`, or a call to the exact `IUnknown` destructor that performs that check. The 24H2 form also destroys a separate stack transform wrapper before consuming the incoming FrameworkElement. Its exact stack address, initialization and adjacent cleanup sequence are checked; extra destructor calls cannot be ignored. Every direct call to the exact release leaf is counted in this form: only the single, null-guarded `[rbp-0x60]` input-temporary cleanup immediately after `TransformToVisual` is accepted. An early direct release of the incoming wrapper, an additional release, or a changed guard is rejected. The release leaf has no unwind record, so it has a separate six-instruction recipe: save and clear the wrapper, select vtable slot 2 (`Release`), and tail-call the exact CFG dispatch pointer. Other functions require `.pdata` boundaries. Missing boundaries do not trigger a generic disassembly fallback.

The source getter has two reviewed register allocations. The newer form keeps the source field in RDI and calls the exact DesktopWindowXamlSource `Content` consumer without overwriting that field pointer. The 24H2 form keeps it in RSI and inlines the Content call through slot 6. Both require the exact ABI getter, null-preserving this adjustment, unique field and direct object flow; the RSI form additionally checks the field dereference, output location, vtable slot and bounded failure branch. These recipes derive the actual offsets: 24H2/LTSC 2024 has element `+0x40`, host-child `+0x18`, child-source `+0x10`; the reviewed 25H2 images have `+0x10`, `+0x38`, `+0x10`. A matching recipe is still only static candidate evidence.

Some exact public PDBs report three XamlExplorerHost functions as undecorated raw names. Only the explicitly listed raw names are accepted, with independent anchors: child vtable slot 8 for the ABI getter, a unique direct call from that getter for its implementation, and factory vtable slot 6 for the creator. The raw implementation name can name both a setter and a getter; the direct-call anchor resolves that overload. The raw creator additionally needs the reviewed argument sequence reaching the exact factory implementation. Conflicting decorated bindings are rejected. `Image.exact()` remains exact and unique, and the tool never invents decorated symbols or performs a global fuzzy-name fallback. Candidate `source.evidence.symbol_bindings` retains the actual raw names and each anchor.

The separate `system_tray_drag_drop_borrowed_coordinates_x64_v1` recipe accepts the exact `DragDropManager::ScreenRectForElement` member signature. Its parameters are RCX opaque manager, RDX hidden RECT result, R8 borrowed FrameworkElement wrapper and R9 HWND; RAX returns the original result pointer. RCX is overwritten before use. The complete bounded body must match one of two reviewed instruction templates: 114 instructions with inline TransformBounds and local null-guarded releases, or 96 instructions with the exact TransformBounds consumer and local IUnknown destructor calls. These are distinct compiler shapes of the same ABI. Every call, branch, register, result store and stack cleanup in each body is checked. Named call targets are resolved independently, and MapWindowPoints must be the actual USER32 import or delay-import slot. The borrowed input is never passed to a destructor or release.

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

The offline tests include argument-register swaps, wrong result/return pointers, wrong IAT or named callee, nonlocal/extra cleanup, changed null branches, caller wrapper/result aliasing, a conversion calling a destructor in place of QueryInterface, wrong QI slot/CFG/inner target, conflicting source aliases, and an E8 byte embedded inside an instruction operand. They use literal instruction fixtures and synthetic PE metadata, with no Windows DLL or PDB required.
