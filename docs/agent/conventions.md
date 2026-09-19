# Hard conventions

Breaking any of these is a regression.

- History, image, and export files go through `write_private` /
  `ensure_private_dir` (0600/0700). Never plain `std::fs::write` on those
  paths — other local users would read clipboard secrets.
- History saves are temp-file + rename so a crash cannot wipe the file.
  Keep the two-step.
- Clipboard pipe reads stay capped via `.take(limit + 1)`; an unbounded
  `read_to_end` lets a hostile clipboard offer OOM the poller.
- Copies made by Clipit are suppressed by a sticky hash (`last_set`)
  checked against both clipboard targets. If you touch `watch` or
  `update`, keep both targets suppressed.
- Image capture probes png → jpeg → gif → webp → bmp; stored bytes are
  re-sniffed by magic (`sniff_format`) and the sender's MIME claim is
  never trusted. The format set is tied to `image` crate features — the
  iced widget decodes exactly what those features enable.
- Paste injection is intentionally absent: Wayland forbids cross-app
  input, and synthetic paste risks landing in the wrong window. README
  documents the rationale; don't "fix" it.
