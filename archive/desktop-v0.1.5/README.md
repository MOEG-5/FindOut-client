# Archived desktop UI: v0.1.5

This directory preserves the last production UI before Monolith, including its
light, dark and retro palettes. `main.rs` is an unchanged snapshot of the Rust
source and embedded Slint UI from tag `v0.1.5` (commit `8e38629`). `screenshot.png`
is the corresponding screenshot from that release.

These files are historical references and are not compiled into the current app.
The complete buildable source, dependencies, packaging and tests remain in the
[v0.1.5 tag](https://github.com/MOEG-5/FindOut-client/tree/v0.1.5).
To inspect or build that release without changing the current checkout:

```sh
git worktree add --detach ../FindOut-client-v0.1.5 v0.1.5
```

Production v0.1.6 and later use `src/monolith.slint`, starting in light mode with
the dark variant available from the tray menu. The mobile web UI is separate.
