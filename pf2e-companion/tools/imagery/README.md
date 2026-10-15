# Lore-imagery pipeline (tools/imagery)

Kirk-artbake-shaped generation pipeline for the lore packs' art (the analysis
is `docs/imagery/DEDUCTION.md`; Part 4 is this design). Provider: PayPerQ
`nano-banana-2-1`, single-provider with a $15 cap (human decision 2026-10-14).
No palette quantization anywhere — full-quality render, then downscale to the
role cap and encode WebP q80 at promotion time.

## Layout

- `styles/<lens>.md` — versioned style blocks (`version: N` in frontmatter,
  cited as `styles/<lens>.md@N` in provenance): palette, medium, `## Prefix` /
  `## Suffix` prompt parts, per-lens do-not curation checklist, and the
  per-lens calibration decisions (DEDUCTION §5.3(3)).
- `specs/v1.json` — the 14 v1 assets (2 covers, 3 handout plates, 7 NPC
  tokens, 2 scene vistas): id, role, aspect/cap_px, alt, brief, `v` version,
  and a `wire` block saying how promotion attaches the art to content.
- `gen.py` — spec → PayPerQ → `staging/<id>-vN.png` (+ `.prompt.txt`) and
  MANIFEST rows. `client_request_id = pf2e-<lens>-<asset>-vN`; retries attach
  to finished generations and never double-bill. Stdlib only.
- `compose_sheet.py` — rebuilds `review.html`, the human contact sheet over
  staging (each card shows the per-lens do-not checklist). Stdlib only.
- `promote.py` — approve/foul/run. `run` finalizes (downscale-cap + WebP q80
  + sha256), copies into `src-tauri/data/content/<lens>/art/…`, injects
  `image` / `image_alt` / `image_provenance` frontmatter per `wire`, refuses
  unapproved or alt-less assets. Deps: Pillow (+ PyYAML for the frontmatter
  validity gate), both in `.venv`.
- `MANIFEST.json` — authoritative ledger: spend, lineage, sha256, status.
- `staging/` — gitignored raws; regenerable via `client_request_id`.
- `.venv/` — gitignored local venv (`uv venv .venv && uv pip install pillow pyyaml`).

## Usage

```bash
export PPQ_API_KEY=sk-…   # env only; never written to disk, never logged
cd tools/imagery
.venv/bin/python gen.py                      # generate all pending v1 assets
.venv/bin/python gen.py --dry-run            # print assembled prompts, no network
.venv/bin/python compose_sheet.py            # (re)build review.html; eyeball the set
.venv/bin/python promote.py approve <id>     # after the checklist passes
.venv/bin/python promote.py foul <id> --note "stray text"
#  → edit the brief, then: .venv/bin/python gen.py --bump <id> && gen.py --only <id>
.venv/bin/python promote.py run              # promote every approved asset
```

Budget: `$15` hard cap lives in `MANIFEST.json` (`meta.cap_usd`); `gen.py`
refuses when cumulative cost reaches it and stops the batch at 80%
(human gate). `--max-spend` can lower the cap for a run, never raise it.

Curation gates (`promote.py run` refuses violations): `status: approved`
(curation pass on `review.html`) and non-empty `alt` (a11y). Foul taxonomy:
text/letters, borders/frames, subject drift, the lens's divine-persons row
(DEDUCTION Part 3.6). Review as a set — "siblings, not cousins".
