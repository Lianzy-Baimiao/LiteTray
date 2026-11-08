# LiteTray

> [!WARNING]
> **Development build with serious compatibility problems, being worked on.** The taskbar clock and tray mirroring load an adapter into the File Explorer process and call undocumented Shell interfaces. When the adapter fails, File Explorer crashes and restarts, and the taskbar and desktop disappear for a moment. Another program's installer telling Explorer to refresh its icons can also trigger this. After you quit LiteTray, the adapter stays in Explorer until Explorer restarts. Do not use it on a computer that needs to stay stable. See [Current limitations](#current-limitations).

[![Windows CI](https://github.com/Lianzy-Baimiao/LiteTray/actions/workflows/ci.yml/badge.svg)](https://github.com/Lianzy-Baimiao/LiteTray/actions/workflows/ci.yml)
[![Download](https://img.shields.io/github/v/release/Lianzy-Baimiao/LiteTray?include_prereleases&label=download)](https://github.com/Lianzy-Baimiao/LiteTray/releases)
[![Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue)](LICENSE)
[![Support on Afdian](https://img.shields.io/badge/Support-Afdian-946ce6)](https://ifdian.net/a/lianzy)

[简体中文](README.md) · **English**

A useful clock, tray and calendar on every monitor. A native Windows desktop app built with Rust, with customizable taskbar clocks, mirrored tray icons, weather and local schedules.

![5.12.0 live secondary taskbar with mirrored icons and a two-line custom clock](docs/images/taskbar-5.12.0.png)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/calendar-5.12.0-en-dark.png">
  <img alt="5.12.0 calendar with lunar dates, festivals, hourly weather and schedules" src="docs/images/calendar-5.12.0-en-light.png">
</picture>

5.12.0 previews, refreshed on 2026-11-08. The taskbar is a live secondary-monitor capture using the user's Chinese clock template. The calendar uses the current native controls with sample weather and schedule data; lunar text and sample entry titles retain their original language. Images follow the page's light or dark theme.

## Download

[**Download the Windows x64 installer**](https://github.com/Lianzy-Baimiao/LiteTray/releases/latest) · [Changelog](CHANGELOG.md)

Download `LiteTray-<version>-Setup-x64.exe` from Releases. Installation is per user and needs no administrator privileges. Settings, schedules and weather cache live in `%LOCALAPPDATA%\LiteTray`; upgrades and uninstall preserve user data.

**Compatibility:** 5.14.0 includes profiles for the **x64 system modules from Windows 11 26300.9711, 26300.9818 and 25H2 26200.8457**, selected by exact module identity. Unmatched modules block the features that depend on them. The 9818 Sandbox and an independently installed 25H2 VM passed real primary-clock text/width updates, restoration and tray preparation/stop checks. Actual input, menus, tray replication, multiple monitors and mixed DPI still need acceptance. Tested binaries and exact coverage are recorded in the [9818 validation record](docs/compatibility-9818.md) and [25H2 validation record](docs/compatibility-8457.md), both in Chinese. LiteTray is still in development and the installer is not digitally signed.

## Features

- **Taskbar clocks:** templates, individual weather fields, fonts, colors, alignment and presets. Width follows content; compact taskbars can use one or two lines.
- **Multiple monitors:** mirror the primary tray's icon order and visibility to secondary displays. Clock replacement and icon mirroring have separate switches.
- **Calendar and weather:** lunar dates, solar terms, popular festivals, Chinese public holidays and adjusted workdays; hourly forecasts, city search and approximate local location, with keyless weather providers.
- **Local schedules:** tasks, events, birthdays, anniversaries and countdowns, including editing directly in the calendar flyout.
- **Alarms and focus:** alarms, Pomodoro sessions, timers and optional sounds. Optional wheel volume control and middle-click mute over the taskbar clock.
- **Windows appearance:** system light/dark theme, accent colors and high contrast; detach and drag the calendar to keep it open.
- **UI languages:** Simplified Chinese and English, following the Windows display language by default, with a manual choice in General. Lunar dates, almanac text and upstream holiday notices retain their original language.

## Getting started

Open settings, choose your displays in the tray page, then enable the taskbar clock. Click the clock to open the calendar. An attached panel closes when you click outside it; a detached panel stays open. Closing settings keeps LiteTray running; use the tray menu to exit.

Open **Settings → Compatibility** to see local module matches, built-in profiles and the recorded scope of acceptance checks. Inspection runs in the background without changing the system, and you can export the displayed report. A disk match still requires runtime checks; it does not confirm every click, menu or multiple-monitor scenario.

[Usage and development guide, Chinese](docs/usage.md) · [Installer guide, Chinese](docs/installer.md) · [Weather templates, Chinese](docs/weather-format.md)

[Compatibility checks and offline reports, Chinese](docs/compatibility.md) · [Windows 11 compatibility plan, Chinese](docs/windows11-compatibility-plan.md)

Choose **General → Language**, then save. Templates, entry titles and notes keep their original text. GitHub does not select a README based on your device language; use the language links at the top. [Language support](docs/languages.md)

## Current limitations

Taskbar integration uses an adapter inside Explorer and some undocumented Shell interfaces. Windows updates can affect compatibility, and adapter faults can affect Explorer. QQ popup placement is a separate experimental option, disabled by default.

A reported crash when clicking with Windows notifications present is still awaiting reproduction and a fix. See the [integration notes, Chinese](docs/taskbar-integration-and-store.md) and [Releases](https://github.com/Lianzy-Baimiao/LiteTray/releases) for known limitations and validation details.

## Development and support

Requires Rust stable and Windows C++ build tools. Build with `tools/Invoke-Cargo.ps1`; package the installer with `tools/Build-Setup.ps1`.

Licensed under the [Apache License 2.0](LICENSE). Third-party components retain their respective licenses.

[Report an issue](https://github.com/Lianzy-Baimiao/LiteTray/issues) · [Support on Afdian](https://ifdian.net/a/lianzy) · [License details](docs/licensing.md) · [Third-party notices](THIRD-PARTY-NOTICES.md)
