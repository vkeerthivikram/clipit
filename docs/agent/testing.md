# Testing rules

- Tests must never touch the real `~/.local/share/clipit` — it is the
  user's live clipboard history. Use `std::env::temp_dir()` paths.
- Pure logic only; no GUI test harness exists. The applet itself cannot be
  launched headless.
- The locale-consistency test reads `i18n/` relative paths, so run cargo
  from the repo root.
- i18n-specific test practices (isolation marks, numeric plural args) are
  covered in [i18n rules](i18n.md).
