# Feedback Rush

Outmaneuver your own echoes in a tight, stylish arena. Every pickup you grab spawns a looping “ghost” that replays your recent movement path. The longer you survive and the more you collect, the busier the arena becomes with your past selves. Phase through danger with a limited meter, chain pickups for bigger scores, and ride the rising threat level as the game accelerates.

- Core loop: move, collect, spawn a ghost of yourself, survive the consequences.
- Unique mechanic: each ghost loops a slice of your recent path at increasing speed, turning your mastery into escalating, personalized hazards.
- Infinite replayability: no two runs are the same; your movement paints the arena.

## Features

- Fast, responsive controls with clean movement and friction.
- Echo/ghost system that loops your own recorded path.
- Scoring, combo (chain pickups), and difficulty scaling over time.
- Phase mechanic: short invulnerability by holding Shift or Space (drains/recovers).
- Procedural retro SFX generated in code (no external assets).
- Minimal, legible visuals and subtle camera shake for juice.
- Dynamic resolution support; UI and arena scale to any window size.
- Main menu with multiple game modes and settings.
- Horror vibe: vignette, flickering specters, and a low drone.

## Controls

- Move: WASD or Arrow Keys
- Phase (invulnerable): Shift or Space (drains meter; slowly recharges)
- Start/Restart: Enter
- Return to Menu: Escape (from Game Over)
- Toggle Fullscreen: F11 (also available under Settings)

## How To Play

- Collect the glowing orbs to score. Each pickup spawns a ghost that loops your recent path.
- Ghosts collide with you. Avoid them or phase through at the cost of your meter.
- Chaining pickups quickly increases your score multiplier.
- Threat level rises as you survive and score, speeding up ghosts and spawning orbs faster.

### Game Modes

- Classic: The baseline experience; endless with balanced scaling.
- Time Attack: 60 seconds. Score as much as you can before the buzzer.
- Nightmare: Faster spawn/speed scaling. Ghosts flicker and fade at distance.

## Build and Run

Requirements:
- Rust toolchain (stable) with `cargo`.
- Linux, macOS, or Windows should work thanks to macroquad.
- On Linux, you’ll need OpenGL + audio (e.g., Mesa, ALSA/PulseAudio).

Build:
- Debug: `cargo run`
- Release: `cargo run --release`

## Releases (Windows/macOS/Linux)

GitHub Actions is set up to build and publish binaries for Linux, Windows, and macOS whenever you push a tag like `v0.1.0`.

Steps:
- Push the repo to GitHub (`compiledkernel-idk/feedback-rush`).
- Create and push a tag: `git tag -a v0.1.0 -m "v0.1.0" && git push --tags`.
- Wait for the “release” workflow to finish. Download artifacts from the Release page.

Artifacts created per-OS:
- Linux: `feedback-rush-vX.Y.Z-Linux.tar.gz`
- Windows: `feedback-rush-vX.Y.Z-Windows.zip`
- macOS: `feedback-rush-vX.Y.Z-macos.tar.gz`

Each archive includes: the `feedback-rush` binary, `README.md`, and `LICENSE`.

## Packaging for Arch (AUR)

A PKGBUILD is included. To publish as an AUR package:

1. Tag a release in your GitHub repo (e.g., `v0.1.0`). The CI will also publish prebuilt archives.
2. Update the `pkgver` in `PKGBUILD` to match.
3. Ensure the `url` and `source` values point to your repo (compiledkernel-idk).
4. In an AUR clone for `feedback-rush`, add the `PKGBUILD` and push.

Local build with `makepkg`:
- `makepkg -si`

This builds with Cargo and installs:
- Binary: `/usr/bin/feedback-rush`
- Docs: `/usr/share/doc/feedback-rush/README.md`
- License: `/usr/share/licenses/feedback-rush/LICENSE`

## Tech Notes

- Game is a single file `src/main.rs` using `macroquad`.
- Uses a fixed update timestep (60 FPS) for consistent physics/history.
- Procedural WAV generation for lightweight SFX via `load_sound_from_bytes`.
- No external assets required; portable and fast to compile.

## License

MIT — see `LICENSE`.
