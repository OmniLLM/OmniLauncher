# Security Policy

## Reporting

Please open a private security advisory via GitHub
(https://github.com/OmniLLM/OmniLauncher/security/advisories/new) for any
vulnerability reports. Avoid filing public issues for security topics.

## Known accepted advisories

Some dependency advisories are tracked but not currently actionable for this
project. They are listed here so reviewers and contributors can see why we
have not acted on them.

### GHSA-wrw7-89jp-8q8g — `glib` 0.18 unsoundness in `VariantStrIter`

- **Severity:** Moderate
- **Affected version in lockfile:** `glib 0.18.5` (transitive)
- **Fixed upstream in:** `glib 0.20.0`
- **Pulled by:** `tauri 2.x` → `tray-icon` → `libappindicator` → `gtk 0.18` →
  `atk 0.18` → `glib 0.18.5`
- **Status: ACCEPTED — not actionable until Tauri migrates to gtk-rs 0.20+.**

The advisory describes unsoundness in `glib::VariantStrIter`'s
`Iterator`/`DoubleEndedIterator` impls (incorrect lifetime / iteration
boundary handling for GVariant string-array iteration).

OmniLauncher does **not** use `glib::Variant` directly anywhere in its source
tree. The crate is reachable only through Tauri's Linux GTK runtime stack,
which does not surface `VariantStrIter` to application code. The vulnerable
code path is therefore not reachable from any input controlled by an end user
or remote actor.

We cannot bump `glib` independently because Tauri 2.11 (the latest stable as
of this writing) pins `gtk-rs 0.18` across its Linux backend (`gtk`, `atk`,
`gdk`, `gio`, `cairo-rs`, `webkit2gtk` v2.0.x). A `[patch.crates-io]` override
to `glib 0.20` would be a hard ABI break against the rest of the gtk-rs 0.18
graph.

We will pick this up automatically the next time Tauri bumps its gtk-rs
dependencies. Track upstream:

- https://github.com/tauri-apps/tauri/issues (search "gtk-rs 0.20" / "glib 0.20")
- https://crates.io/crates/tauri/versions

If a tauri release ships with gtk-rs 0.20+, run `cargo update -p glib` and
this advisory will close on the next dependabot scan.
