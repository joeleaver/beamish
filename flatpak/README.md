# Flatpak / Flathub packaging

Beamish builds as a Flatpak against the **GNOME 49** runtime. The GNOME runtime
is used (rather than the bare freedesktop one) because the `system-tray` feature
pulls in GTK 3 — the tray itself runs on the pure-Rust `ksni`
(StatusNotifierItem) backend, but GTK is still linked in and the GNOME runtime
provides it.

## Files

| File | Purpose |
| --- | --- |
| `io.github.joeleaver.beamish.yml` | flatpak-builder manifest |
| `io.github.joeleaver.beamish.metainfo.xml` | AppStream metadata (Flathub requires this) |
| `io.github.joeleaver.beamish.desktop` | Desktop entry (app-id-named; `Icon=`/`StartupWMClass=` use the app-id) |
| `cargo-sources.json` | Vendored Cargo deps for an **offline** build (generated, see below) |

## Build dependencies (one-time)

```sh
flatpak install -y flathub \
  org.gnome.Platform//49 org.gnome.Sdk//49 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
```

## Build & install locally

From the repo root:

```sh
flatpak-builder --user --force-clean --disable-rofiles-fuse \
  --install --repo=.flatpak/repo .flatpak/build \
  flatpak/io.github.joeleaver.beamish.yml

flatpak run io.github.joeleaver.beamish
```

The manifest's app source is a **local** `file://` git checkout of this repo's
`main` branch, so commit any source changes before rebuilding. The manifest,
`metainfo`, `desktop`, and `cargo-sources.json` are read from disk and can be
edited without committing.

> Build dirs live under `.flatpak/` and `.flatpak-builder/` (git-ignored). Keep
> them on the same filesystem (don't point the build dir at `/tmp`, which is
> tmpfs here) or flatpak-builder errors about the state dir.

## Regenerating `cargo-sources.json`

Needed whenever `Cargo.lock` changes. Uses
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools):

```sh
curl -fsSLO https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
uv run flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json
```

## Notable manifest details

- **protoc** is installed as a build-time-only module (rqs_lib's `build.rs` runs
  `prost-build`, which shells out to `protoc`). `PROTOC` points at it and it is
  removed from the final image via `cleanup`.
- **libxdo** is bundled because `muda` (the menu library) hard-links `-lxdo` and
  the GNOME runtime does not ship it.
- The build is fully offline (`cargo --offline`); all crates come from
  `cargo-sources.json`. flatpak-builder downloads those pinned sources up front.

## Toward Flathub submission

For a Flathub PR, change the `beamish` module's source from the local `file://`
git URL to the public repo at a tagged release, e.g.:

```yaml
sources:
  - type: git
    url: https://github.com/joeleaver/beamish.git
    tag: v0.1.2
    commit: <full-sha-of-tag>
  - cargo-sources.json
  - type: file
    path: io.github.joeleaver.beamish.metainfo.xml
  - type: file
    path: io.github.joeleaver.beamish.desktop
```

Still needed before submitting:

- At least one **screenshot** hosted in the repo (the metainfo references
  `assets/screenshots/receive.png`).
- Drop `--env=BEAMISH_APP_ID=...` only if the app's default app-id is changed to
  the flatpak id; today the app reads `BEAMISH_APP_ID` so native builds keep the
  short `beamish` id.
