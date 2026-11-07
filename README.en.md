# LiteTray

[![Windows CI](https://github.com/Lianzy-Baimiao/LiteTray/actions/workflows/ci.yml/badge.svg)](https://github.com/Lianzy-Baimiao/LiteTray/actions/workflows/ci.yml)
[![Download](https://img.shields.io/github/v/release/Lianzy-Baimiao/LiteTray?include_prereleases&label=download)](https://github.com/Lianzy-Baimiao/LiteTray/releases)
[![License pending](https://img.shields.io/badge/License-pending-lightgrey)](docs/licensing.md)
[![Support on Afdian](https://img.shields.io/badge/Support-Afdian-946ce6)](https://ifdian.net/a/lianzy)

[简体中文](README.md) · **English**

A useful clock, tray and calendar on every monitor. A native Windows desktop app built with Rust, with customizable taskbar clocks, mirrored tray icons, weather and local schedules.

![Live secondary taskbar with mirrored icons and a two-line custom clock](docs/images/taskbar-secondary.png)

![Calendar, lunar dates and weather panel](docs/images/panel-light.png)

The taskbar image is a live capture. The calendar is a native UI rendering preview with sample weather and schedule data; these previews show the Chinese interface.

## Download

[**Download the Windows x64 installer**](https://github.com/Lianzy-Baimiao/LiteTray/releases/latest) · [Changelog](CHANGELOG.md)

Download `LiteTray-<version>-Setup-x64.exe` from Releases. Installation is per user and needs no administrator privileges. Settings, schedules and weather cache live in `%LOCALAPPDATA%\LiteTray`; upgrades and uninstall preserve user data.

**Compatibility:** native clock replacement and tray mirroring currently support only the verified **Windows 11 26300.9711 x64 system modules**. Other versions are rejected. LiteTray is still in development and the installer is not digitally signed.

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

[Usage and development guide, Chinese](docs/usage.md) · [Installer guide, Chinese](docs/installer.md) · [Weather templates, Chinese](docs/weather-format.md)

Choose **General → Language**, then save. Templates, entry titles and notes keep their original text. GitHub does not select a README based on your device language; use the language links at the top. [Language support](docs/languages.md)

## Current limitations

Taskbar integration uses an adapter inside Explorer and some undocumented Shell interfaces. Windows updates can affect compatibility, and adapter faults can affect Explorer. QQ popup placement is a separate experimental option, disabled by default.

A reported crash when clicking with Windows notifications present is still awaiting reproduction and a fix. See the [integration notes, Chinese](docs/taskbar-integration-and-store.md) and [Releases](https://github.com/Lianzy-Baimiao/LiteTray/releases) for known limitations and validation details.

## Development and support

Requires Rust stable and Windows C++ build tools. Build with `tools/Invoke-Cargo.ps1`; package the installer with `tools/Build-Setup.ps1`.

[Report an issue](https://github.com/Lianzy-Baimiao/LiteTray/issues) · [Support on Afdian](https://ifdian.net/a/lianzy) · [License status](docs/licensing.md) · [Third-party notices](THIRD-PARTY-NOTICES.md)
