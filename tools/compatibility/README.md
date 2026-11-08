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

The implementation currently recognizes the bounded `TaskbarHost`/`IXamlExplorerHostChildWindow` field chain and the three SystemTray coordinate method signatures found in the reviewed x64 images. New instruction shapes must be inspected and added as explicit recipes; do not relax a check merely to obtain a candidate. Promotion requires code review, an implemented module/ABI variant where necessary, and feature-specific VM live results. This tool deliberately has no promotion or `--force` option.

The screen-rectangle function is checked for both reviewed owned-value cleanup paths: an inline null check followed by `IUnknown::unconditional_release_ref`, or a call to the exact `IUnknown` destructor that performs that check. The release leaf has no unwind record, so it has a separate six-instruction recipe: save and clear the wrapper, select vtable slot 2 (`Release`), and tail-call the exact CFG dispatch pointer. Other functions require `.pdata` boundaries. Missing boundaries do not trigger a generic disassembly fallback.

The input root is a collection boundary, not proof that files came from one installed OS. `coherent_installation_verified` remains `false`, including when every static recipe passes. Distinct modules may legitimately have different component version numbers. A known production profile and isolated loaded-image checks must validate the complete set; never pick one module's offsets from another candidate. The tool records actual filename casing; compare Windows paths case-insensitively, without treating different basenames as equivalent.

The independently reviewed 26200.8457 exact profile is included in the default registry; see [the three-set comparison](../../docs/windows11-commonality.md) and [its VM validation record](../../docs/compatibility-8457.md). Recorded acceptance covers the module/layout chain, primary clock update and stop/restore, and tray preparation and stop. Pointer interactions, tray/model mirroring, multiple monitors, and mixed DPI remain separate validation scopes. This static candidate output does not inherit those live results or become a production profile.

Run the rejection tests without system access or downloads:

```powershell
python -m unittest discover -s tools/compatibility -p test_evidence.py -v
```

For a regression against a known image, run with its exact cached PDBs and compare `candidate_layouts` to the reviewed profile. Keep that local result separate from live acceptance evidence. Source images must be obtained and verified independently; a PDB match identifies a binary but is not proof of installation-media provenance.
