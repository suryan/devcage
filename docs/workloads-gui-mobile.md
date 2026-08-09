# Workloads: GUI and mobile

**Status:** **Phase 3 appendix** — not core product, not v0.1.
**Core product focus:** easy isolation for AI agent development (`agent` / `offline` / `host`). See [product_requirements.md](./product_requirements.md).

**Scope:** How DevCage *might* later handle iOS, Android, and desktop GUI without over-claiming isolation. Until then: cage the agent; keep Xcode / Studio / emulators on the host.

See also: [architecture.md](./architecture.md), [usage.md](./usage.md).

---

## Problem

CLI tools and many agent workflows need **file I/O + network**. GUI and mobile toolchains also need:

- display servers (X11, Wayland, or macOS windowing)
- host IPC daemons (e.g. CoreSimulator via Mach / launchd)
- hypervisors / KVM (Android emulator acceleration)
- GPU devices (`/dev/dri`, host Metal outside the Linux cage)

A **Linux container on macOS** (MVP backend) cannot host Xcode or the iOS Simulator. DevCage therefore uses **two strategies** and **explicit elevated profiles** — never the default.

| Strategy | Idea | Best for |
|----------|------|----------|
| **Selective service bridging** (“hole-punching”) | Keep host services where they must run; allow only specific sockets, ports, and paths into the session | iOS Simulator, host ADB, interactive display |
| **Headless virtual display** | Render inside the cage (Xvfb / private compositor); agent inspects via screenshots / optional noVNC | Autonomous GUI testing on Linux |

**Bridges are trusted host surface.** They are smaller than mounting full `$HOME`, but still powerful (sim control, `adb`, live X11). Residual risk must stay documented. **Fail closed** if required bridges are missing — never silently fall back to unrestricted host.

---

## 1. iOS Simulator (`ios-sim`)

### Challenge

On macOS, the iOS Simulator is **not** a VM you put in Docker. It runs as **native macOS processes** against simulated frameworks, coordinated by host services such as CoreSimulator and launchd / Mach IPC.

```text
┌─────────────────────────────────────────────────────────┐
│              DevCage session (host-mac backend)         │
│   Agent / xcodebuild / simctl                           │
└──────────────────────────┬──────────────────────────────┘
                           │ Mach IPC / UNIX sockets
                           │ + explicit path allowlist
                           ▼
┌─────────────────────────────────────────────────────────┐
│                       Host macOS                        │
│   CoreSimulatorService ──► Simulator window             │
│   Default: ~/.ssh, ~/.aws, personal files not mounted   │
│   Keychain / signing: OFF unless explicit high-trust    │
└─────────────────────────────────────────────────────────┘
```

### How DevCage handles it

**Do not** run Xcode inside a generic Linux container.

| Piece | Intent |
|-------|--------|
| **Backend** | **`native-mac` / host process policy** — not the container backend. If only Docker is available, `ios-sim` **fails closed** with a clear error. |
| **IPC / sockets** | Bridge only what is required for `xcrun simctl` / CoreSimulator (exact set is implementation-defined; expect iteration). |
| **Directories** | Explicit RW/RO mounts: project workspace; selected CoreSimulator and Xcode/SDK paths; project DerivedData as needed — **not** wholesale `$HOME`. |
| **Secrets** | Default: no `~/.ssh`, `~/.gnupg`, browser profiles, cloud cred dirs. |
| **Signing / Keychain** | **Default off.** Real device + codesign often needs Keychain — ship as optional high-trust tier (e.g. `ios-sim-sign` or `--allow-keychain`), never silent. |

### Capability tiers

| Tier | Capabilities | Trust |
|------|--------------|--------|
| **A — sim agent** | Build for sim, `simctl` boot/install/launch, project + DerivedData + sim paths | Elevated vs `offline`; no Keychain by default |
| **B — sign / device** | Codesign, provisioning, physical device | High trust; document Keychain and broader host coupling |

### Residual risk (honest)

Even without mounting `~/.ssh`, an agent that can drive CoreSimulator can:

- install and run software on the simulator
- read much of **simulator** app data and capture UI
- consume disk/CPU under CoreSimulator paths
- affect the host via any full host binaries the profile still invokes (Xcode tooling)

**Prompt injection** can still rewrite the **mounted** Xcode project. DevCage reduces host-home blast radius; it does not make the app repo immutable.

---

## 2. Android

Two approaches; pick by interactive vs autonomous and by OS.

### Approach A: Host-delegated ADB (`android-adb`) — **recommended default**

Emulator or physical device stays on the **host**. Build and deploy logic run in the cage (typically **container** backend).

```text
Cage:  ./gradlew assembleDebug  →  adb install …
                    │
                    │ TCP to host loopback ADB (e.g. 127.0.0.1:5037
                    │ or host.docker.internal:5037)
                    ▼
Host:  adb server  →  emulator / USB device UI
```

| Rule | Detail |
|------|--------|
| Bridge | **ADB TCP only** (loopback on host; do not expose ADB on the LAN by default) |
| In cage | Project workspace; optional SDK/JDK per profile; **not** full host home |
| Out of cage | Emulator window, Studio GUI, raw USB (USB is mediated by host `adb`) |
| Trust | Treat ADB like a **limited break-glass** channel: debug bridges can pull app data, reverse ports, etc. |

### Approach B: In-cage headless emulator (`android-headless`)

For autonomous agents that should not touch the desktop display:

- **Linux** container/host with **KVM** (`/dev/kvm`) when available
- Emulator flags such as `-no-window -no-audio`
- Agent drives UI via `adb shell input` and `adb exec-out screencap`

| Platform | Fit |
|----------|-----|
| Linux + KVM | Primary target |
| macOS Docker | **Not** the default — nested virt / emulator-in-Docker is fragile; prefer `android-adb` |

---

## 3. Desktop GUI (Linux and macOS)

### Linux: display bridging vs virtual display

| Profile (intent) | Display | GPU | Trust | Use case |
|------------------|---------|-----|-------|----------|
| `gui-headless` | Xvfb or private compositor inside cage; optional noVNC | optional | **Default for agents** | Autonomous UI test; no hijacking the developer monitor |
| `gui-x11` / `gui-wayland` | Real user session socket (`/tmp/.X11-unix`, Wayland runtime) | optional `/dev/dri` | Higher — live display is ambient-authority-ish | Interactive desktop app dev |

**Live X11** can expose other clients on the same display (classic X security issues). Prefer **headless / private display** for untrusted agents; use live display only in an explicit interactive profile.

Optional GPU: mount `/dev/dri` for OpenGL/Vulkan when the profile allows.

### Headless virtual display (agent-first)

```text
┌─────────────────────────────────────────────────────────────┐
│                       DevCage session                       │
│   GUI app ──► virtual display (Xvfb / private compositor)   │
│                      │                                      │
│                      ▼                                      │
│   Agent ◄── screencap / video / DOM  (+ optional noVNC)     │
└─────────────────────────────────────────────────────────────┘
```

- App renders off the main monitor.
- Agent evaluates layout via captures.
- Optional human peek via **localhost-bound** noVNC (or similar); do not publish VNC broadly without an explicit net policy.

### macOS native GUI (AppKit / SwiftUI / Xcode UI)

X11-in-Linux-container does **not** equal native macOS UI. Options:

1. **Hybrid:** cage the agent / package installs; run the native app or IDE on the host against the same project mount.
2. **Host backend** with FS allowlists (same family as `ios-sim`).
3. Later **VM** backend if full disposable desktop isolation is required.

---

## Profile summary

| Profile | Backend | Allowed (intent) | Blocked (intent) | Use case |
|---------|---------|------------------|------------------|----------|
| `ios-sim` | **host-mac** | CoreSimulator-related paths/IPC, project, DerivedData, toolchain paths as listed | Full home; ssh/aws dirs; **Keychain default off** | iOS agent + simctl (Tier A) |
| `android-adb` | container | Project; optional SDK; **ADB to host loopback** | Host home; raw USB in cage | Default Android interactive / agent deploy |
| `android-headless` | container **Linux+KVM** | Local emulator + adb; headless | Host display | Autonomous Android on Linux |
| `gui-headless` | container / bwrap | Virtual display; optional noVNC | Real host display; host secrets | Agent visual UI testing |
| `gui-x11` / `gui-wayland` | container / bwrap | User display socket; optional GPU | Host home | Interactive Linux desktop dev |

All of the above are **explicit opt-in**. Special device / display / ADB access is **never** default (see FR-7).

---

## Security rules (all GUI/mobile profiles)

1. **Bridges are trusted host surface** — document power and residual risk next to each profile.
2. **Fail closed** if required sockets, devices, or host helpers are missing.
3. **No Keychain / signing by default** on `ios-sim`.
4. **ADB and live X11** are break-glass-adjacent (compare to mounting `docker.sock`).
5. **Prompt injection** still rewrites mounted projects and can drive sim/device/UI **within** the bridge.
6. Prefer **loopback-only** for ADB and VNC unless net policy explicitly allows more.

---

## Roadmap alignment

| Phase | GUI / mobile |
|-------|----------------|
| **v0.1** | Not required. Container MVP: agents, packages, offline/host net, project RW. Hybrid mobile/GUI (cage agent, host IDE/sim) works without special profiles. |
| **v0.2+** | Net allowlist helps controlled ADB/VNC ports; `gui-headless` / `android-adb` become realistic. |
| **Post-MVP** | `native-mac` + `ios-sim`; Linux `gui-*`; `android-headless` where KVM exists; optional signing tier. |

MVP users doing iOS/Android today: run agents under generic profiles for **repo + installs**, and keep **Xcode / Studio / emulators on the host** until these profiles ship.
