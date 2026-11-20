# Project Context

Корень репо (CWD-якорь): `/home/sche/Documents/programming/Piano/vst_midi_curves`.

VST3-плагин и standalone-приложение на Rust (nih-plug + egui): преобразование
velocity MIDI-нот через редактируемую кривую (как «Кривые» в Photoshop).

Карта проекта: открывай нужное по задаче.
- `src/lib.rs` — VST3-плагин (nih-plug): `MidiCurvesPlugin`, параметры,
  hi-res velocity буфер (`VstHiResBuffer`), GUI плагина, `nih_export_vst3!`;
- `src/main.rs` — standalone-приложение (eframe): `MidiCurvesApp`, отрисовка
  графика и точек, панель пресетов;
- `src/curve/` — математика кривых: `bezier.rs`, `dual_curve.rs` (двойная
  кривая), `interpolation.rs`, `control_point.rs`;
- `src/midi/` — MIDI для standalone: `ports.rs`, `device_manager.rs`
  (горячее подключение устройств), `processor.rs`; `src/midi_simple.rs` —
  упрощённый менеджер MIDI;
- `src/presets.rs` — пресеты кривых (`PresetManager`), `src/settings.rs` —
  настройки приложения;
- `tests/` — интеграционные тесты (`curve_tests.rs`, `standalone_tests.rs`).
- Сборка: `./build.sh` / `build_linux.sh` / `build_macos.sh` / `build_windows.sh`,
  `cargo build --release` (lib → VST3, bin `midi_curves` → standalone).
  `build/midi_curves_v0.1.0/` — собранный релиз (артефакт в git).
- Документация: `README.md`, `ARCHITECTURE.md`, `BUILD_GUIDE.md`.
Осознай контекст проекта. (Fedora)

Рабочая папка сессии: `.work/` в корне репо (в `.gitignore`). Скрипты, промежуточные
данные, заметки и прочий скрэтчпад, которые должны пережить обрыв сессии или
перезагрузку компьютера, складывать туда, а не в `/tmp` или системный скрэтчпад.
Очищать только в конце работы и только по запросу пользователя. В git не добавлять.

# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

## 5. Read Before Edit

**Never edit a file you haven't read in the current session.**

- Before any edit, read the file in this session — even if you "remember" it from a prior session. Names rename, lines shift, branches diverge.
- Before writing a new file, check the directory to make sure you're not duplicating or overwriting something.
- Paths recalled from memory must be re-verified against the current working tree before use.

## 6. Destructive Actions — Confirm First

**Irreversible operations need explicit consent, every time.**

Always confirm before:
- `rm -rf`, deleting files/directories, `git clean -fd`
- `git reset --hard`, `git checkout -- <file>`, `git branch -D`
- Force push (`--force`, `--force-with-lease`) — never on main/master
- Dropping DB tables, truncating data, killing processes
- Overwriting uncommitted changes
- Bypassing hooks (`--no-verify`, `--no-gpg-sign`)

Prior approval for one destructive action does not authorize others. Scope of consent = exactly what was asked.

When blocked, diagnose the cause — don't reach for a destructive shortcut to make the error disappear.

## 7. Worktree / Sandbox Path Discipline

**When CWD is a git worktree or any sandbox with sibling copies of the same repo, files with identical names exist in multiple places. Do not confuse them.**

- At session start, check `AGENTS.md` / `CLAUDE.md` / `README.md` for a CWD anchor. Compare with `pwd`. If they disagree, stop and ask.
- All paths in `grep` / `find` / `Read` / `Edit` are relative, starting from `.`. No `../`, no absolute paths leaving CWD, no `find` from `$HOME` or parent directories without explicit permission.
- In `grep` / `find` output, always look at the **full match path**, not just the matching line — the hit may come from a sibling worktree.
- To step outside CWD (sibling worktree, main repo, another project), say so explicitly and get consent first.
- Re-read paths remembered from prior sessions against current CWD before acting — the worktree/branch may be different now.
- Inside a worktree, `git log` shows that branch's history, not master/main. State the revision explicitly when it matters.

The confusion starts on **read**, not on write — this rule disciplines reading too.

## 8. Формулы в объяснениях

**Каждая формула — с расшифровкой; формулу с параметрами — показать графиком.**

- Под формулой кратко: что значит каждая переменная и константа (смысл, единицы или масштаб модели), какому имени в коде она соответствует (константа, поле, макрос — с путём к файлу), расшифровка сокращений и аббревиатур (ZOH, T60, PDL…).
- Если параметры формулы есть смысл покрутить — запустить `fplot` (`~/.local/bin/fplot`, справка `fplot -h`): одна команда Bash, без файлов, окно с ползунками отрывается само. Пример: `fplot 'n*f0*sqrt(1+B*n^2)' n=1:40 f0=27.5:4186:440:log B=1e-5:1e-2:4e-4:log --n`. Диапазоны ползунков — реалистичные для задачи.
- Скриншот окна из сессии не снять; для самопроверки — `--save <scratchpad>/x.png` и просмотреть картинку.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

