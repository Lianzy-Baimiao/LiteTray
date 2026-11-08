#!/usr/bin/env python3
"""Generate offline candidate evidence; never register or activate a profile."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sys

from evidence import EvidenceError, Image, acquire_pdb, enumerate_symbols, require, within
from layouts import host_layout, source_layout, topology_clues, tray_layout

ROLES = ("taskbar", "shell", "tray")
DEFAULTS = {"taskbar": ("System32/taskbar.dll",), "shell": ("System32/twinui.pcshell.dll",)}
TRAY_LOCATIONS = tuple(f"SystemApps/{package}/{name}" for package in (
    "MicrosoftWindows.Client.Core_cw5n1h2txyewy", "MicrosoftWindows.Client.CBS_cw5n1h2txyewy")
    for name in ("SystemTray.dll", "Taskbar.View.dll", "ExplorerExtensions.dll"))
LIVE_REQUIREMENTS = [
    "Review the candidate recipe, module roles, decorated symbols and ABI before any production code change.",
    "Validate loaded-image identity, path, instruction prefixes and source ownership in an isolated VM.",
    "Validate real taskbar topology, thread, HWND and XamlRoot ownership; file strings are insufficient.",
    "Validate clock/input/tray features separately, including stop/restore and failure recovery.",
    "Record multi-monitor, DPI, menus and third-party hover coverage separately from single-screen smoke checks.",
]


def save(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def choices(root, overrides):
    selected, inventory, errors = {}, {}, []
    for role in ROLES:
        candidates = DEFAULTS.get(role, TRAY_LOCATIONS)
        present = [relative for relative in candidates if within(root, relative).is_file()]
        inventory[role] = {"searched_paths": list(candidates), "present_paths": present}
        if role in overrides:
            selected[role] = overrides[role]
        elif len(present) == 1:
            selected[role] = present[0]
        else:
            errors.append({"stage": "module_selection", "role": role,
                           "error": f"Expected one module, found {len(present)}; provide an explicit --module {role}=relative/path.dll"})
    return selected, inventory, errors


def run(args):
    require(os.name == "nt" and sys.maxsize > 2**32, "Use native 64-bit Python on Windows for DbgHelp")
    root = args.system_root.resolve(strict=True)
    require(root.is_dir(), "--system-root must be a Windows directory or extracted Windows file tree")
    output = args.output.resolve()
    cache = (args.symbol_cache or output / "symbols").resolve()
    production = Path(__file__).resolve().parents[2] / "crates" / "adapter-protocol" / "profiles"
    for path in (output, cache):
        require(not path.is_relative_to(root), "Output/cache must be outside the input Windows tree")
        require(not path.is_relative_to(production), "Developer evidence cannot be written into production profiles")
    require(not output.exists(), "Output directory already exists; use a new path to preserve previous evidence")
    overrides = {}
    for value in args.module:
        require("=" in value, "--module syntax is ROLE=relative/path.dll")
        role, relative = value.split("=", 1)
        require(role in ROLES and role not in overrides, "Unknown or repeated module role")
        within(root, relative)
        overrides[role] = relative.replace("\\", "/")
    output.mkdir(parents=True)
    selected, inventory, blockers = choices(root, overrides)
    images = {}
    for role, relative in selected.items():
        try:
            images[role] = Image(role, root, relative)
        except Exception as error:
            blockers.append({"stage": "image_identity", "role": role, "error": str(error)})
    require(len({image.path for image in images.values()}) == len(images), "One file cannot impersonate two module roles")
    require(len({image.sha256 for image in images.values()}) == len(images), "Identical images cannot impersonate two module roles")
    proofs = {}
    # Network downloads can overlap. DbgHelp itself remains strictly serial.
    with ThreadPoolExecutor(max_workers=3) as workers:
        futures = {workers.submit(acquire_pdb, image, cache, args.offline): role for role, image in images.items()}
        for future in as_completed(futures):
            role = futures[future]
            try:
                proofs[role] = future.result()
                print(json.dumps({"stage": "pdb_ready", "role": role, "bytes": proofs[role][1]["size"]}), flush=True)
            except Exception as error:
                blockers.append({"stage": "exact_pdb", "role": role, "error": str(error)})
    for role in ROLES:
        if role not in proofs:
            continue
        image = images[role]
        try:
            enumerate_symbols(image, proofs[role][0])
            save(output / f"{role}-symbols.json", {"module": image.module(), "pdb": proofs[role][1],
                 "dbghelp": image.symbol_proof, "symbols": image.symbols})
        except Exception as error:
            blockers.append({"stage": "exact_symbols", "role": role, "error": str(error)})
    layouts, assembly = {}, []
    stages = (
        ("host", ("taskbar",), lambda: host_layout(images["taskbar"], assembly)),
        ("source", ("taskbar", "shell"), lambda: source_layout(images["taskbar"], images["shell"], assembly)),
        ("tray", ("tray",), lambda: tray_layout(images["tray"], assembly)),
    )
    for name, dependencies, inspect in stages:
        if not all(role in images and images[role].symbol_proof for role in dependencies):
            blockers.append({"stage": name, "error": "Exact module/PDB evidence unavailable"})
            continue
        try:
            layouts[name] = inspect()
        except EvidenceError as error:
            blockers.append({"stage": name, "error": str(error)})
    for image in images.values():
        try:
            image.recheck()
        except EvidenceError as error:
            blockers.append({"stage": "final_identity", "role": image.role, "error": str(error)})
    topology = topology_clues(images["tray"]) if "tray" in images else None
    production_blockers = ["Candidate evidence has no automatic promotion path", "VM live validation is required"]
    if "tray" in images and images["tray"].path.name.casefold() != "systemtray.dll":
        production_blockers.append("The default production registry has no entry for this alternate module; an explicit lab variant and isolated live validation are required")
    result = {
        "schema": "litetray.compatibility.candidate.v1", "status": "blocked" if blockers else "static_candidate",
        "production_ready": False, "registry_modified": False,
        "label": args.label, "label_used_for_matching": False,
        "collected_at": datetime.now(timezone.utc).isoformat(), "system_root": str(root),
        "scope": {"disk_only": True, "explorer_accessed": False, "target_dlls_loaded": False,
                  "runtime_topology_checked": False, "coherent_installation_verified": False,
                  "network_allowed": not args.offline},
        "module_selection": inventory, "modules": {role: image.module() for role, image in images.items()},
        "pdb_evidence": {role: {**proof[1], "dbghelp": images[role].symbol_proof} for role, proof in proofs.items()},
        "candidate_layouts": layouts, "topology_clues": topology,
        "analysis_blockers": blockers, "production_blockers": production_blockers,
        "required_live_validation": LIVE_REQUIREMENTS,
    }
    save(output / "candidate.json", result)
    save(output / "disassembly.json", assembly)
    with (output / "disassembly.txt").open("x", encoding="utf-8") as stream:
        for function in assembly:
            stream.write(f"\n{function['module_role']} +0x{function['rva']:X}: {function['symbol']}\n")
            for ins in function["instructions"]:
                stream.write(f"{ins['rva']:08X} {ins['bytes']:32s} {ins['instruction']}\n")
    print(json.dumps({"status": result["status"], "output": str(output), "production_ready": False,
                      "analysis_blockers": blockers, "production_blockers": production_blockers}, ensure_ascii=False), flush=True)
    return 2 if blockers else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--system-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New evidence directory; never overwritten")
    parser.add_argument("--module", action="append", default=[], metavar="ROLE=PATH",
                        help="Explicit taskbar, shell or tray role with its real relative filename")
    parser.add_argument("--symbol-cache", type=Path, help="Reusable exact PDB cache; defaults under output")
    parser.add_argument("--offline", action="store_true", help="Reject missing PDBs instead of downloading")
    parser.add_argument("--label", default=None, help="Human inventory label only, never a compatibility selector")
    args = parser.parse_args()
    try:
        return run(args)
    except (EvidenceError, OSError, ValueError) as error:
        print(json.dumps({"status": "rejected", "error": str(error)}, ensure_ascii=False), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
