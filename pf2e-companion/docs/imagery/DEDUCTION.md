# DEDUCTION — Generated Imagery for pf2e-companion Lore Content

Status: analysis only. No implementation, no generation, no app changes proposed
for merge without a follow-up plan. Compiled 2026-10-14 from a read-only pass
over `/home/ethan/code/kirk/` (the working art pipeline) and this repo.

TL;DR: kirk's pipeline is a small, honest, transferable skeleton — OpenAI-shaped
image API call + versioned prompt style-blocks + git-staged ledgered candidates
+ human eyeball curation + promote-on-approval. pf2e-companion has ~115
imageable entries across five lens packs plus 2 adventures; the highest-value
first batch is **14 assets** (2 adventure covers, 3 handout plates, 7 NPC token
portraits, 2 scene establishing shots) which, under PayPerQ's current
nano-banana free window, cost **$0.00** and worst-case **under $2** afterward.
The hard problems are not technical: they are per-lens doctrinal image rules
(the Reformed pack must not depict the Godhead; Orthodox icons are liturgical
objects) and prompt-drift control. Every UI seam either already exists as a
stub (`scene.background_entity_id` → `sceneBgUrl`) or degrades gracefully.

---

## Part 1 — How kirk does it (deduced from the repo)

Sources: `/home/ethan/code/kirk/docs/ART-PIPELINE.md` (research + plan,
superseded in part), `/home/ethan/code/kirk/docs/ART-STATUS.md` (live state,
2026-10-14), `/home/ethan/code/kirk/tools/artbake/*.py`,
`/home/ethan/code/kirk/tools/artbake/MANIFEST.json`,
`/home/ethan/code/kirk/art/roster-staging/` (79 KiB of staged candidates).

### 1.1 Provider and models (verified in scripts, not just docs)

- **Endpoint:** `https://api.ppq.ai/v1/images/generations` (PayPerQ,
  OpenAI-images-API-shaped). Hard-coded in `tools/artbake/ppq_gen.py:7` and
  `ppq_roster.py:9`. Confirms ART-STATUS §1's claim.
- **Auth:** `PPQ_API_KEY` env var only → `Authorization: Bearer <key>` header
  (`ppq_gen.py:8,46`). Never written to disk; ART-STATUS §6: "user's personal
  store only… grep before pushes."
- **Models that won the bake-off** (ART-STATUS §1, confirmed `ppq_roster.py:14`):
  - `nano-banana-2-1` (Google) — manifest shows `cost_usd: 0.0` on every cell.
    Per ppq.ai blog (2026-10-05), the whole nano-banana family is **$0 on
    PayPerQ until November 2026** for accounts with a balance; published
    post-promo price ≈ $0.0336/1K image (benchlm.ai price scrape, 2026-10-06).
  - `gpt-image-1.5` (OpenAI) with `quality: "low"` — manifest shows
    `cost_usd: 0.023` per image.
- **Payload** (`ppq_roster.py:90-93`): `{model, prompt, n: 1, size: "1:1",
  output_format: "png", client_request_id}` plus per-model extras.
- **Response:** `data[0].url` (a 24h-signed URL, fetched immediately and
  archived), plus a `cost` field that the scripts record verbatim.
- **Idempotency:** `client_request_id` = `kirk-portfolio-<model>-<subject>-vN`.
  ART-STATUS §2: "retries attach to finished generations and never
  double-bill." A per-subject `ID_VERSION` map (`ppq_roster.py:84-87`) bumps
  `-vN` when a prompt changes materially, which retires the old cell and forces
  regeneration — that map is the entire "prompt versioning" system.

### 1.2 Prompt assembly

Three layers, concatenated (`ppq_roster.py:90`):

1. `STYLE` — a subject-agnostic constant prefix: medium descriptor ("chunky
   16-bit JRPG pixel-art game sprite"), the palette as *named colors*,
   composition rules (centered, full-body silhouette, solid background
   `#0B0E1B`), and a hard negative list: **"absolutely no text, no letters,
   no watermark, no border, no card frame"** (`ppq_roster.py:16-22`).
2. `SUBJECTS[subject]` — one hand-written brief per subject, derived only
   from the game's lore strings.
3. Register tail — `CHAMP_TAIL` ("humble, dignified medieval Christian-crusader
   allegory") vs `BOSS_TAIL` ("grotesque allegorical sin-spawn… not gory, no
   real people, no occult symbols") (`ppq_roster.py:24-26`). The tail is how
   the pipeline encodes *tone/IP guardrails per content class*.

### 1.3 Batch → staging → curation → promote

- **Batch driver:** `ppq_roster.py` iterates `MODELS × SUBJECTS`; skips any
  cell whose `raw.png` exists (cheap idempotency); on success writes
  `raw.png` + quantized `q64/q128.png` under `portfolio/<model>/<subject>/`;
  failures (HTTP or otherwise) are collected, printed, and never fatal.
  Moderation edge case observed: one boss prompt got an upstream HTTP 400;
  fix was rewording, not retrying (ART-STATUS §2).
- **Ledger:** `tools/artbake/MANIFEST.json` — one row per `<model>/<subject>`
  cell: `raw_bytes`, `q64_bytes`, `q128_bytes`, `client_request_id`,
  `model` (provider-qualified as `ppq:<model>`), `subject`, `src_dims`,
  `cost_usd`. This is the authoritative spend/lineage record.
- **Raw rasters are NOT in git** (regenerable via the idempotency keys);
  only quantized production candidates land in-repo under
  `art/roster-staging/<model>/<subject>/{q64,q128}.png`.
- **Human review:** `gen_site.py` builds a local `index.html` gallery
  (RAW vs q64 vs q128 per cell, cost badges) served with
  `python3 -m http.server`. ART-STATUS §4.1 names the curation loop: "eyeball
  fouls (text/border/subject drift), re-prompt individual cells… bump `-vN`
  id." Fouls have a working taxonomy: stray text, borders/frames, subject
  drift. ART-PIPELINE §5 adds: anchor 2 subjects first (hero + boss showcase),
  approve them to death, then treat them as style references for the rest;
  "19 cousins instead of 19 siblings" is the named failure mode.
- **Deps:** python3 stdlib + Pillow only. `quant.py` is a pure-stdlib PNG
  reader/writer doing block-average downscale + nearest-RGB palette snap to a
  16-color master palette.

### 1.4 Costs observed (from ART-STATUS + MANIFEST)

Bake-off $0.204 + portfolio batch $0.115 ≈ **$0.32** spent; estimated ~$0.35
to finish the 40-cell roster. Human time dominates: 15–60 min cleanup per
sprite, ~10 h per full roster (ART-PIPELINE §4–5).

### 1.5 The transferable skeleton (what pf2e-companion should copy)

1. **Provider call**: one script-shaped function — env-keyed bearer auth,
   `client_request_id` idempotency with a per-subject version bump, immediate
   archival of the signed URL payload, cost recorded into a JSON ledger.
2. **Prompt = STYLE prefix + subject brief + register tail**, where STYLE
   lives in a version-controlled file and carries the negative list.
3. **Staging with per-cell dirs + MANIFEST.json**; raws out of git, curated
   finals in git (here: candidates; for us: promoted finals).
4. **A human curation gate**: contact-sheet/gallery, named foul taxonomy,
   re-prompt loop that bumps the id version. Art is approved as *sets*
   ("siblings, not cousins").
5. **Placeholder-swap discipline**: "art is parallel work that can never
   block code" (ART-PIPELINE §0); UI ships with fallbacks; images swap in.

---

## Part 2 — Asset census (measured from `src-tauri/data/content/`)

Counts below were measured with `find` over the five pack dirs (129 files
total: 112 markdown entries, 5 `pack.toml`, 12 `.statblock.json` sidecars).
Every markdown entry here is a candidate; `pack.toml` files are metadata and
are excluded.

| Asset class | L | C | R | P | O | Total | Files ground truth |
|---|---|---|---|---|---|---|---|
| Deity entries (`deities/yhwh.md`) | 1 | 1 | 1 | 1 | 1 | **5** | `*/deities/yhwh.md` (each + 1 statblock sidecar) |
| Saints / exemplars / archangels (`saints/`) | 4 | 20 | 6 | 6 | 9 | **45** | e.g. `orthodox/saints/` has 7 named archangels + 2 doctors; `catholic/saints/` has 12 apostles + 4 archangels + Mary, Joseph, Francis, Thérèse |
| Cosmology planes (`cosmology/`, `type: plane`) | 6 | 7 | 5 | 6 | 7 | **31** | reformed lacks abyss/purgatory entries; orthodox adds aerial-toll-houses |
| Class reskins (`classes/`, `type: class-reskin`) | 5 | 5 | 5 | 5 | 6 | **26** | orthodox adds monk |
| Adventures (`lewisian/adventures/`) | 2 | 0 | 0 | 0 | 0 | **2** | widows-lamp (levels 1–2), roof-of-mercy (3–4) |
| Handouts (adventure frontmatter `handouts:`) | 3 | 0 | 0 | 0 | 0 | **3** | widows: `letter` (gm), `journal` (all); roof: `paralytics-prayer` (all) |
| NPC statblock sidecars (adventure) | 7 | 0 | 0 | 0 | 0 | **7** | widows: lamplighter, debt-collector, widow-mara, thug ×(×2 in scene); roof: wandering-teacher, paralytic-elian, temple-scribe (×2 in scene) |
| Reference (`reference/`) | 0 | 0 | 1 | 1 | 1 | **3** | covenants, charismata, theosis |
| Vision/read-aloud scenes (adventure frontmatter `scenes:`) | 6 | 0 | 0 | 0 | 0 | **6** | 3 per adventure, each with `read_aloud` text |

### 2.1 Imageable assets, UI seam, v1 priority

| # | Asset | UI seam (verified) | v1? | n |
|---|---|---|---|---|
| 1 | **Adventure covers** (widows-lamp, roof-of-mercy) | `src/routes/table/play/+page.svelte` — adventure slug currently renders as a text badge (`.adventure-badge`, l. 306-309); a cover panel belongs in the load flow + `src/routes/entity/[id]/+page.svelte` hero for `type: adventure` | **yes** | 2 |
| 2 | **Handout plates** (collector's letter, Mara's journal, paralytic's prayer) | `table/play/+page.svelte` handouts tab, `.handout-card` (l. 522-540) — body is literally `markdown rendering TBD`; an image above the body is the reveal moment | **yes** | 3 |
| 3 | **NPC token portraits** (7 sidecar NPCs → square 256px) | `table/play/+page.svelte` `.token` squares (l. 493-511) + party/initiative lists; entity rail | **yes** | 7 |
| 4 | **Scene establishing shots** (rain-soaked tenement; crowded house/roof) | `table/play/+page.svelte` scene tab — **stub already wired**: `background_entity_id` in the session doc, `sceneBgUrl` + `.grid-bg.placeholder` (l. 286-291, 485-490) with the comment "In a real app, this would fetch the entity's image URL" | **yes (2 only)** | 2 |
| 5 | Saint/archangel portraits (45) | `entity/[id]/+page.svelte` hero/rail; search-hit thumb (`src/routes/+page.svelte` l. 101-117) later | batch 2 (12 anchor figures), rest batch 3 | 45 |
| 6 | Cosmology plane vistas (31) | entity hero banner | batch 3 (642px-wide vista) | 31 |
| 7 | Class reskin art (26) | entity hero/rail; future class-picker art | batch 3 | 26 |
| 8 | Deity "images" (5) | entity hero — **special rules per lens (Part 3)**; several lenses get symbols or vistas, never persons | batch 3, symbols only | 5 |
| 9 | Reference art (3) | none v1 | no | 3 |
| 10 | App/marketing splash, store key art | README, store listings (Tier 2 §7), `src-tauri/icons/` | deferred to Tier-2 store pass | ~4 |

**v1 total: 14 images.** Grand total if fully arted: ~120 images (+derived
search thumbs at 128px, auto-downscaled, not separately generated).

### 2.2 Non-goals (explicit)

- **No palette quantization / pixel-art treatment** — user decision; kirk's
  `quant.py` does not transfer.
- **No per-spell / per-feat / per-domain icon sets** (none of those entity
  types exist in the bundled packs; search filter chips list them but the
  corpus has no entries).
- **No battle-map cartography** — the scene grid is abstract 32px cells
  (`--cell-size: 32px`), no map layer exists; vistas are atmosphere, not maps.
- **No animation, no video** (Veo exists on the same provider — not needed).
- **No user-vault image pipeline in v1** — vault ingest
  (`src-tauri/src/vault.rs`) is markdown-only; custom-content art is a later
  design, not this one.
- **No Paizo Community Use Package art remixing, no AI-styled "Paizo cover
  look"** — Product Identity ban (Golarion/Absalom/deity marks) per
  `docs/SHIPPING.md` Tier 2 §8; all art is original to this project.
- **No generated text-in-image** (illuminated initials, inscriptions,
  captions) — kirk's fouls list shows models hallucinate lettering; every
  style block carries the no-text negative. Handout typography stays CSS.
- **No depicting real, named modern denominations'/ministries' likenesses**,
  no polemical imagery of one lens aimed at another.

---

## Part 3 — Per-lens image flavor

"Each flavor of lore gets its own image flavor." Each direction below is
codifiable as a **subject-agnostic style block** (prefix + palette + composition
rules + do-not list) that concatenates with a per-asset subject brief exactly
like kirk's `STYLE + SUBJECT + TAIL`. The project-wide guardrails bind every
lens first: reverent and ecumenical; no mockery; no Paizo Product Identity
(no Absalom/Golarion landmarks, iconics, or deity marks — `docs/SHIPPING.md`
Tier 2 §8); nothing readable as one real tradition scoring points off another.

A shared **tail** for every lens (adapted from kirk's hard-won negative list):

> …reverent devotional-fantasy illustration, dignified, no gore. Absolutely
> no text, no letters, no inscriptions, no watermark, no border, no frame,
> no logo. No copyrighted fantasy-franchise visual references. No real
> identifiable living or recent persons.

(A relaxable exception for the catholic/orthodox blocks: pseudo-Greek or
pseudo-Latin *halo inscriptions* are part of the medium's grammar — but the
kirk fouls list shows models botch real lettering, so v1 keeps "no text"
everywhere and accepts abstract glyph-marks only where the medium demands
them; review each such cell by hand.)

### 3.1 `lewisian` — storybook ink-and-wash plate

- **Register** (grounded in `lewisian/pack.toml`, both adventure files'
  "Mere-Christianity posture… plays in the hallway" notes): the
  mid-century British children's-book plate — Pauline Baynes for Narnia,
  early Tolkien illustration. Pen-and-ink linework with translucent
  watercolor wash; visible paper texture; intimate readerly scale.
- **Palette (named):** warm cream paper, iron-gall ink brown-black, lamp-lit
  amber, moss green, slate grey-blue rain, oxblood red accents, one small
  point of pale gold light per image (the Lewisian "Light" leitmotif —
  `the-widows-lamp` frontmatter makes the lamp the literal subject).
- **Composition:** vignette with natural white margin falloff; figures small
  within landscape/interior; weather and light doing emotional work.
- **Depicts:** scenes, places, ordinary people, symbolic objects (the lamp,
  the torn roof, the stretcher). Adventures live here, so this block carries
  the highest-stakes v1 assets.
- **Never depicts:** any Person of the Godhead as a figure (the Teacher in
  roof-of-mercy is rendered from behind, at distance, mid-gesture, or as
  light-and-crowd reaction — the camera is Mark 2's crowd, not a portrait
  lens); denominational vestments; mock medievalism; anything cutesy.
- **Style-block prefix (draft):**
  > Vintage mid-century British storybook illustration plate, ink and
  > watercolor on warm cream paper, fine pen linework with translucent
  > washes, gentle vignette edges fading to paper, intimate cozy scale,
  > lamp-lit amber and moss-green and slate-grey palette with a single small
  > pale-gold light source. Subject (never a frontal portrait of any divine
  > person):

### 3.2 `catholic` — illuminated manuscript / stained glass / baroque altarpiece

- **Register** (grounded in `catholic/pack.toml`: "closest in tone to
  Tolkien's *Silmarillion* and a medieval-Catholic vibe"; nine-choir
  Pseudo-Dionysian hierarchy): the trecento-to-baroque devotional register —
  gold-ground illumination marginalia for small slots, stained-glass leaded
  color for saints, chiaroscuro altarpiece drama for vistas.
- **Palette:** deep ultramarine, gold leaf, crimson lake, bone parchment,
  emerald; Marian blue reserved for `mary-queen-of-heaven`.
- **Composition:** frontal iconic saint with attribute and halo
  (St. Michael: spear/dragon/scales per Rev 12:7 and the entry's own
  mechanics `catholic/saints/michael.md`); arched frame *within* the image
  (the style itself, not a drawn border — the negative list still bans a
  literal frame edge); vistas as apse-scale heavenly/hellish drama
  (`catholic/cosmology/purgatory.md` as luminous ascent, never horror).
- **Depicts:** Christ (the Incarnation is the tradition's warrant for
  images), Mary with hyperdulia restraint, saints, angels per the nine-choir
  descriptions, sacramental objects, church architecture.
- **Never depicts:** the Father as an old man (the "Ancient of Days" type —
  contested even within the tradition; use the hand of God, the Chi-Rho,
  the Lamb, or the dove instead); the Trinity as three men; irreverent
  contemporary dress; horror-gore hell; clerical satire; televangelist
  kitsch; anything sexualized.
- **Style-block prefix (draft):**
  > Devotional panel painting in the late-medieval-to-baroque Catholic
  > register: egg-tempera and gold-leaf luminosity or leaded stained-glass
  > color, deep ultramarine and crimson and gold on parchment, solemn frontal
  > hieratic composition, each holy figure identified by a traditional
  > attribute, thin gold halo, drapery in deep folds. Never depict God the
  > Father as a person. Subject:

### 3.3 `orthodox` — Byzantine icon (tempera, gold, inverse perspective)

- **Register** (grounded in `orthodox/pack.toml`: "late-Byzantine visual
  register (Hagia Sophia, gold leaf, royal porphyry)" and
  `orthodox/saints/gregory-palamas.md`'s Tabor light as "physical,
  perceptible, but uncreated"): the icon — egg tempera on gold-ground panel,
  flat inverse perspective, no cast shadows, no naturalistic depth.
- **Critical sensitivity: icons are liturgical objects, not decoration.**
  The pack must not ship pastiche "icons" that parody veneration. The
  rendering should read as *an icon photographed respectfully*: straight-on,
  full gold ground, no cropping of faces. Where the app context is playful
  (e.g. a token), prefer the architectural-vista or symbol variant.
- **Palette:** burnished gold ground, porphyry purple-red, lapis/olive
  drapery, white highlights as uncreated-light assistolâ, dark cypress green.
- **Depicts:** Christ incarnate (the canonical warrant), the Theotokos (the
  seven-archangel roster: Michael, Gabriel, Raphael, Uriel, Selaphiel,
  Jegudiel, Barachiel — all have entries in `orthodox/saints/`), frontal
  half-length figures with halo, hands in traditional gesture.
- **Never depicts:** **God the Father — absolute** (Orthodox canon: only the
  incarnate Son may be imaged; the Great Moscow Synod of 1663 condemned
  Father/Ancient-of-Days imagery); the Holy Spirit except as the dove at
  Theophany-type scenes; narrative comic dynamism, 3D naturalistic shading,
  motion blur, gore; crucifixion gore naturalism (icons are stylized even
  there).
- **Style-block prefix (draft):**
  > A Byzantine Orthodox icon photographed straight-on: egg tempera on
  > gold-leaf panel, flat inverse perspective, no cast shadow, serene frontal
  > half-length figure with halo, drapery in porphyry and lapis with gold
  > assiste highlights, eyes large and quiet, dark cypress and gold palette.
  > God the Father is never depicted; the Spirit appears only as a dove.
  > Subject:

### 3.4 `reformed` — Reformation woodcut / engraving (symbols over persons)

- **Register** (grounded hard in `reformed/deities/yhwh.md`: "worshipped
  without images (the regulative principle)… sacred colors are black and
  white — the Reformation pulpit gown, the geneva bands, the unornamented
  sanctuary — which is itself a confession against the visual maximalism that
  earlier traditions deploy"; sources cite Calvin *Institutes* I.11-12,
  Heidelberg Catechism Lord's Day 35 Q.96-98): the 16th-century print —
  Geneva Bible woodcut, pamphlet engraving, later steel-engraved frontispiece.
  Black line on cream paper, cross-hatching, flat mid-grey tone.
- **Critical sensitivity: the Second Commandment is load-bearing here.**
  Westminster LC 109 forbids images of any Person of the Godhead "inwardly
  in our mind, or outwardly in any kind of image." Unlike the other four
  lenses, this pack's visual *theology* is its austerity — the restraint is
  the flavor, not a limitation.
- **Depicts (favored subjects):** symbols and scenery — the open Bible above
  a plain cross (the entry's own designated religious symbol,
  `reformed/deities/yhwh.md` l. 61), the burning bush unconsumed (the
  Reformed/Presbyterian emblem), an anchor, a crown of thorns on an empty
  cross, lamplight on an open book, covenant-rainbow over landscape, the
  five solae as abstract wordless seals; cosmology entries as wordless
  engraved vistas (heaven as throne-occupied-but-unseen radiance, New
  Jerusalem as city-of-light architecture); biblical witnesses (Abraham,
  Moses, David, Paul, the two archangels) rendered as *narrative scene*
  woodcuts rather than devotional portraits — figures within an event,
  never frontal/iconic.
- **Never depicts:** any Person of the Godhead (no Father-figure, no Christ
  portrait, no dove-as-portrait — symbols like the open Bible carry the
  deity entry instead); halos; crucifixes with corpus; baroque or
  iconographic conventions that would read as "Catholic art"; color (the
  entry says the colors are black and white — honor it);
  satirical/polemical anti-Catholic imagery (the historic woodcut tradition
  is full of it — that polemic is exactly what the project's ecumenical
  guardrail forbids).
- **Style-block prefix (draft):**
  > A Reformation-era woodcut engraving, black ink on cream laid paper,
  > dense cross-hatching, stark black-and-white with one warm india-ink
  > wash at most, quiet symbolic still-life or narrative scene composition,
  > no devotional portrait framing, no divine person depicted, no halos.
  > Subject:

### 3.5 `pentecostal` — contemporary prophetic-vision acrylic

- **Register** (grounded in `pentecostal/pack.toml`: "active spiritual-warfare
  cosmology in the Frank Peretti *This Present Darkness* key" and
  `pentecostal/classes/oracle.md`'s "charismatic without health-and-wealth
  triumphalism"): the modern prophetic/worship-art genre — large acrylic or
  digital painting, saturated color over darkness, visible energetic
  brushwork, light as the main actor. This is the one lens where full
  emotional saturation is *correct*; the pack description names fire, wind,
  and the dove.
- **Palette:** midnight indigo ground lit by flame orange, gold-white
  glory-light, dove white, emerald oasis, crimson; never pastel.
- **Depicts:** the Spirit's scriptural signs — tongues-of-fire light above a
  diverse gathered crowd (Acts 2 framing), the descending dove, wind
  streaming through a meeting room's curtains, Elijah-type confrontation
  (fire from heaven on the altar — `pentecostal/saints/elijah.md`), Daniel's
  apocalyptic beasts as *visions* clearly marked visionary, worshipping
  raised hands in silhouette against glory.
- **Never depicts:** the Father as a person; exploitation imagery
  (offering buckets, private jets, stage smoke, merchandise tables —
  health-and-wealth kitsch is the pack's own named anti-type);
  demonic-gore horror (warfare imagery stays in the light-vs-dark
  Peretti-with-hope register, angels win); denominational signage or real
  megachurch interiors; anything mocking ecstatic practice (the project
  guardrail: no mockery).
- **Style-block prefix (draft):**
  > Contemporary prophetic-vision painting, acrylic/digital with bold
  > expressive brushwork, midnight-indigo darkness pierced by flame-orange
  > and gold-white glory light, worship and spiritual-warfare themes with
  > hope dominant, light itself as the visible protagonist, no textural
  > kitsch. Never depict God the Father as a person. Subject:

### 3.6 Cross-lens matrix

| Lens | Medium | Divine persons | Figure register | Light motif |
|---|---|---|---|---|
| lewisian | ink-and-wash storybook | never frontal; light/crowd only | small figures in scene | one pale-gold lamp/light |
| catholic | illumination / stained glass / baroque | Christ yes, Father as hand/Lamb/Chi-Rho, Spirit as dove | frontal iconic + attribute | gold leaf |
| orthodox | Byzantine icon | Christ yes, **Father never**, Spirit as dove only | frontal hieratic icon | gold ground + assiste |
| reformed | woodcut engraving | **none** — symbols/scenery carry doctrine | narrative scene or symbolic still-life, no halos | lamplight on the Book |
| pentecostal | prophetic acrylic | Father never; Spirit as fire/wind/dove | crowd in motion, silhouettes toward light | glory-light dominates |

The matrix row "Divine persons" is the audit checklist for the curation gate:
any staged image that violates its lens's row is an automatic reject regardless
of aesthetic quality.

---

## Part 4 — Integration + pipeline plan

### 4.1 Where image files live (bundle-size math)

Measured inputs:

- The whole content corpus today is **700 KiB** of markdown+JSON
  (`du -sh src-tauri/data/content`).
- `content::load_bundled_packs` embeds the entire tree via
  `static CONTENT_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data/content")`
  (`src-tauri/src/content.rs:23`) — **anything placed under
  `data/content/` is already inside the binary with zero loader changes.**
- kirk's raws prove naive PNG sizing: 0.96–1.7 MB per 1024–1254px PNG
  (`tools/artbake/MANIFEST.json`). 120 images of that size = 120–200 MB —
  unacceptable in a desktop/mobile bundle.

Recommended sizing (downscale-only, never quantize; encode WebP ≈ q80 — all
Tauri webviews render it; lossless PNG only for the 14 v1 assets if Pillow
lacks WebP on the build box):

| Role | Cap (px) | Est. bytes | n | Subtotal |
|---|---|---|---|---|
| Adventure cover (4:5-ish, shown large) | 1024 long edge | 120–220 KB | 2 | ~0.4 MB |
| Handout plate | 768 | 80–150 KB | 3 | ~0.35 MB |
| NPC token portrait (square) | 256 | 15–35 KB | 7 | ~0.2 MB |
| Scene establishing vista (16:9) | 1280 wide | 150–280 KB | 6* | ~1.3 MB |
| Saint/exemplar portrait (square) | 512 | 50–110 KB | 45 | ~3.7 MB |
| Class reskin art | 768 | 80–150 KB | 26 | ~3 MB |
| Plane vista | 1280 wide | 150–280 KB | 31 | ~6.7 MB |
| Deity symbol/vista | 768 | 60–120 KB | 5 | ~0.5 MB |
| **Full corpus** | | | **~125** | **≈ 16 MB worst case** |

*v1 generates only 2 of the 6 scene vistas.

Verdict: **bundle inside the binary under
`src-tauri/data/content/<lens>/art/`**. v1's 14 images ≈ ≤1 MB total; full
corpus ≈ 10–16 MB — heavy but tolerable for a desktop+mobile installer, and
this app is explicitly offline-first ("It runs offline", `src/routes/about/
+page.svelte` l. 24-26), which eliminates lazy-fetch as the primary path.
Vault/data-dir placement was rejected: bundled packs are read-only and
app-state-resident by design (`content.rs` header comment), and a second
resolution path would fork every UI seam. Keep the checksum discipline:
manifest records SHA-256 of each promoted file.

### 4.2 Frontmatter schema addition (conventions-compatible)

New optional keys, co-designed with the existing `license_provenance` +
`sources` pattern (`content.rs:26-38` flattens all extras into the searchable
frontmatter JSON; `EntityDetail.frontmatter` in
`src-tauri/src/commands.rs:96-106` already exposes the whole map to the UI —
**discovery needs zero Rust changes for markdown entities**):

```yaml
image: art/saints/michael.webp            # relative to the pack dir
image_alt: "Icon of the Archangel Michael, frontal, gold ground, spear
           lowered, scales in the left hand"   # required when image is set
image_provenance:
  generator: ppq                          # provider short-name
  model: nano-banana-2-1
  style_block: styles/catholic.md@1       # versioned style file reference
  client_request_id: pf2e-catholic-michael-v1   # idempotency/lineage key
  generated_at: 2026-10-20
  curated: true                           # human approved via gallery pass
  promoted_at: 2026-10-21
```

`promote.py` refuses to write `image:` without `image_alt` (a11y gate) and
without `curated: true` (curation gate).

Adventure handouts need one additive struct field: `HandoutDef` in
`src-tauri/src/adventure.rs:39-47` gains `image: Option<String>` +
`image_alt: Option<String>` (both `#[serde(default)]`, pure extension);
`session/doc.rs:197-202` handout registration carries it into the doc. Handout
*bodies* are still "rendering TBD" in the player UI
(`table/play/+page.svelte:533`) — image rides the same small contract
extension whenever bodies land. Scene vistas need no new frontmatter:
`scenes[]` already carry ids; the convention `art/adventures/<slug>/scene-N.webp`
+ the existing `background_entity_id` resolver (§4.3) is enough.

### 4.3 UI seams (exact, with graceful fallbacks)

1. **`src/routes/entity/[id]/+page.svelte`** — hero: if
   `entity.frontmatter.image` resolves (new `entity_image` IPC returns bytes
   → object URL), render a right-floating portrait card (portrait classes:
   saint/npc/deity-symbol) or a banner (adventure cover / plane vista) above
   `.layout`. Fallback: today's header unchanged. Alt = `image_alt`.
2. **`src/routes/table/play/+page.svelte`** —
   - Load flow: cover panel beside "Load Widow's Lamp" GM action + a
     dismissible cover banner when `snapshot.meta.adventure_slug` first
     appears (the current `.adventure-badge` line keeps working as its
     text fallback).
   - Handouts tab: image above handout body inside `.handout-card`.
     Fallback: current muted placeholder line.
   - Scene tab: the stub `$effect` at l. 286-291 resolves
     `background_entity_id` + scene index → vista URL; `.grid-bg.placeholder`
     stays the fallback. Tokens: `.token` gains a portrait background-image
     when the bound NPC has one; label remains below for legibility.
3. **`src/routes/+page.svelte`** — search-hit thumbnails: 40px rounded square
   left of each `.hit-link`; fallback is a CSS monogram of the entity type
   (first letter chip, same shape). Defer to batch 2 — hit lists already work.
4. **`src/lib/Statblock.svelte`** — no v1 change; a statblock with a thin
   portrait strip can come later. `src/lib/Sources.svelte` unchanged.

New IPC (one command): `get_entity_image(pack_rel_path) -> Result<string>`
returning base64 data-URL from `CONTENT_DIR`, with a denylist guard against
`..` traversal; the frontend caches URLs per id. ~40 lines of Rust + a Svelte
`EntityImage` helper component. This is deliberately the *only* new IPC:
every seam funnels through it.

### 4.4 The generation pipeline (this repo, kirk-shaped)

```
tools/artbake/
  styles/<lens>.md        # the five Part-3 style blocks, versioned (@N)
  specs/v1-covers.yaml    # per-asset briefs: id, lens, role, size, brief, refs
  specs/v2-heroes.yaml …  # later batches
  generate.py             # reads a spec file → PPQ call → staging (skips
                          # existing finals; client_request_id pf2e-<lens>-
                          # <asset>-vN; appends MANIFEST.json row w/ cost)
  finalize.py             # Pillow: downscale-to-role-cap + webp encode +
                          # sha256 → staging/<asset>/final.webp
  review_gallery.py       # static index.html of staged cells (kirk's
                          # gen_site.py pattern), foul-checklist checkboxes
  promote.py              # copies approved finals into
                          # src-tauri/data/content/<lens>/art/…, injects
                          # image/image_alt/image_provenance frontmatter,
                          # refuses uncurated or alt-less assets
  MANIFEST.json           # ledger: spec-hash, client_request_id, model,
                          # cost_usd, src_dims, final sha256, status
art/staging/              # GITIGNORED (kirk keeps raws out of git; our
                          # promoted finals are the in-git artifact)
```

Rules inherited from kirk: raws never enter git (regenerable via
`client_request_id`); a prompt change bumps `-vN`; fouls (text, borders,
subject drift, **per-lens matrix violation**) → re-prompt, never patch in an
editor; review happens on the contact sheet *as a set* ("siblings, not
cousins"); anchor-first — the two adventure covers are canon-anchors for
lewisian, one approved saint per other lens before any batch-2 work.

### 4.5 Rollout order + cost

| Batch | Contents | n | $0-window | Post-promo worst case* |
|---|---|---|---|---|
| v1 | covers 2, handouts 3, NPC tokens 7, scene vistas 2 | 14 | $0.00 | ≈ $0.50–1.40 |
| v2 | per-lens hero anchors (5), adventure-linked saints, remaining vistas | ~18 | — | ≈ $0.70–1.90 |
| v3 | rest of saints, classes, planes, deity symbols | ~90 | — | ≈ $3.50–9.50 |

\* Published single-call prices: nano-banana-2-1 ≈ $0.0378/2K-list (observed
$0.00 on PPQ until Nov 2026), gpt-image-1.5 low $0.023 (observed in kirk's
MANIFEST). Assumes higher quality tier + ~40% regenerate/foul overhead. The
real budget is human review time: est. 10–20 min per asset (style + doctrine
check against the matrix), ≈ 4–5 h for v1.

### 4.6 Risks (honest)

1. **Doctrinal missteps (highest severity).** A stained-glass-style Reformed
   entry or a Father-figure in the orthodox pack would be worse than no image.
   Mitigation: the Part-3 matrix is the curation gate; per-lens do-not lists
   are in the style block itself; v1's 14 assets are all lewisian (the
   safest, most forgiving register) precisely so the review loop can be
   rehearsed on low-risk ground. Recommend a named human reviewer per lens
   for batches 2+.
2. **Licensing / CUP posture (must land in the Tier-2 §8 pass).** Paizo's
   2023 AI-content ban covers *Paizo products and the Infinite
   marketplaces* — it does not govern independent ORC/Compatibility-License
   works that use no Paizo art. Searches found **no** CUP clause about AI
   art in third-party compatible apps (CUP governs use of Paizo Material,
   which we don't reuse). Still: (a) pure AI output is broadly
   non-copyrightable (USCO position, per kirk §7) — fine for our purposes,
   disclosed; (b) store submissions (Steam/itch, Tier 2 §7) require AI
   disclosure; (c) Paizo-community sentiment is strongly anti-AI — the
   About page should state provenance plainly ("generated art, human-directed
   and curated") both for honesty and as a values signal. Flag unresolved
   until the licensing pass reviews it.
3. **Bundle bloat.** Bounded by §4.1 math; enforce in CI: a size check on
   `src-tauri/data/content/**/art/*` (fail over role caps) and a total-dir
   ceiling. Re-encode, never upsize.
4. **Prompt drift between batches.** Style blocks live in versioned files
   referenced from `image_provenance.style_block`; a batch must cite one
   version; client_request_id versioning per kirk.
5. **Placeholder-swap discipline.** Art never blocks code: every seam
   (§4.3) has a shipped fallback *first*, exactly as kirk ships text
   fallbacks. If v1 art slips, the app is unchanged.
6. **Cost creep.** Trivial at observed prices (worst-case full corpus
   < $12); cap via `--max-spend` flag in generate.py reading MANIFEST sums.
7. **Upstream moderation fouls.** kirk hit an HTTP 400 moderation rejection
   on a "sloth" boss prompt. Religious-weird adjacency ("tongues of fire",
   "abyss") may false-positive; fix is rewording, never retry-spam.
8. **Reverence drift in regenerate loops.** The temptation to iterate toward
   "cool" erodes "reverent" (kirk's cousins-not-siblings failure, doctrinal
   edition). The curation checklist includes a one-line "would a thoughtful
   believer of this tradition recognize this as affectionate rather than
   kitsch?" test per lens.

---

## Part 5 — Acceptance

### 5.1 The v1 asset list (first batch worth generating)

14 assets, all lewisian-lens (lowest doctrinal risk, highest table
visibility). Subject briefs are deferred to `tools/artbake/specs/v1-*.yaml`
at implementation time; ids and file targets are fixed here.

| # | Asset | Target file (promoted) | Role cap |
|---|---|---|---|
| 1 | Cover: The Widow's Lamp — rain-hammered tenement, one amber window | `lewisian/art/adventures/widows-lamp/cover.webp` | 1024px |
| 2 | Cover: The Roof of Mercy — crowded house, figures on the roof | `lewisian/art/adventures/roof-of-mercy/cover.webp` | 1024px |
| 3 | Handout plate: The Collector's Letter — black-wax seal, coin in wax | `lewisian/art/adventures/widows-lamp/handout-letter.webp` | 768px |
| 4 | Handout plate: Mara's Journal — water-stained notebook, lamp glow | `lewisian/art/adventures/widows-lamp/handout-journal.webp` | 768px |
| 5 | Handout plate: The Paralytic's Prayer — crumpled vellum, sepia ink | `lewisian/art/adventures/roof-of-mercy/handout-prayer.webp` | 768px |
| 6 | Token: Widow Mara | `lewisian/art/adventures/widows-lamp/npc-widow-mara.webp` | 256px |
| 7 | Token: The Lamplighter | `lewisian/art/adventures/widows-lamp/npc-lamplighter.webp` | 256px |
| 8 | Token: Debt Collector (iron-ledger mask register) | `lewisian/art/adventures/widows-lamp/npc-debt-collector.webp` | 256px |
| 9 | Token: Thug (reusable ×2) | `lewisian/art/adventures/widows-lamp/npc-thug.webp` | 256px |
| 10 | Token: Wandering Teacher (rear/distant per §3.1) | `lewisian/art/adventures/roof-of-mercy/npc-wandering-teacher.webp` | 256px |
| 11 | Token: Paralytic Elian | `lewisian/art/adventures/roof-of-mercy/npc-paralytic-elian.webp` | 256px |
| 12 | Token: Temple Scribe (××2) | `lewisian/art/adventures/roof-of-mercy/npc-temple-scribe.webp` | 256px |
| 13 | Vista: Dock Ward in rain, lamp in window (scene-1 establishing) | `lewisian/art/adventures/widows-lamp/scene-1.webp` | 1280px |
| 14 | Vista: The courtyard house, crowd pressing in (scene-1 establishing) | `lewisian/art/adventures/roof-of-mercy/scene-1.webp` | 1280px |

All 14 use the lewisian style block (§3.1) + shared tail. The two covers are
the canon anchors: they get full iterate-until-approved treatment before any
other cell runs.

### 5.2 The five style blocks (one-line recap; full drafts in Part 3)

- **lewisian** → mid-century British storybook ink-and-wash plate; lamp-lit
  amber on cream paper; Godhead never frontal, light does the talking.
- **catholic** → illuminated-manuscript / stained-glass / baroque devotional;
  gold + ultramarine; Christ yes, Father only as hand/Lamb/Chi-Rho.
- **orthodox** → Byzantine egg-tempera icon, gold ground, inverse perspective;
  Christ yes, Father *never*, Spirit only as dove; icons are liturgical, not
  decoration.
- **reformed** → Reformation black-line woodcut on cream paper; symbols and
  scenery carry doctrine; *no divine person, no halos, no color maximalism*.
- **pentecostal** → contemporary prophetic-vision acrylic; glory-light over
  midnight; fire/wind/dove yes, Father never, prosperity-kitsch never.

### 5.3 Questions for the human

1. **Budget ceiling** — worst-case full corpus is < $12 + several evenings of
   review; is the nano-banana $0 window (until Nov 2026) the intended v1
   window, and do we keep gpt-image-1.5 as the paid second opinion per cell
   or single-provider?
2. **Provider confirmation** — PayPerQ `nano-banana-2-1` as primary matches
   kirk's bake-off; do you want a fresh two-model bake-off for the *large
   format* roles (covers/vistas at 1024–1280px), since kirk only measured
   128px sprite quality?
3. **Per-lens calibration sign-off** — specifically: (a) is the Reformed
   "no divine persons, symbols only" reading the one you want shipped, and
   should Reformed *saints* be narrative woodcut scenes or symbolic
   still-lifes per entry? (b) Orthodox: straight-on icon panels in a game
   app — comfortable, or prefer icon-in-architecture (apse wall) treatment
   for contextual distance? (c) Catholic: confirmation that stained-glass
   (not photo-baroque) is the saint register; (d) Pentecostal: how
   contemporary may dress/interiors be — present-day or timeless-robes?
4. **Reviewer roster** — who is the named doctrinal reviewer per lens for
   batches 2+? (v1 is all lewisian and can proceed on maintainer review.)
5. **alt-text language** — image_alt strings ship in English only, matching
   the corpus; confirm no i18n plan changes that.
6. **CLAUDE/AGENTS note** — should the placeholder-swap rule ("art never
   blocks code; seams ship with fallbacks") be recorded in the repo's
   agent-instructions file when the pipeline lands?

### 5.4 What this doc deliberately did not do

Read-only everywhere in kirk; no prompts written, no generation run, no spec
files created, no Rust/Svelte changes proposed beyond identifying seams and
the one additive `HandoutDef`/`get_entity_image` extension. Evidence-weight
honesty: Paizo-CUP-vs-AI (risk 4.6/2) is the one place where the record is
incomplete — researched 2026-10-14, flagged for the Tier-2 §8 licensing pass
rather than resolved here.

Explicit deltas for pf2e-companion: **no palette quantization** (kirk's
`quant.py` step is dropped; the user wants full-quality PNGs, downscaled at
most); add `image_alt` accessibility text (this repo has an a11y pass on
record, README "Phase 11… a11y cleanup"); different output surfaces (desktop
Tauri app, not wasm web).

---

**imagery v1 generated** (2026-10-14; art promoted 2026-10-15) — all phases done.

- **Pipeline (Phase A):** `tools/imagery/` — versioned style blocks for all
  five lenses (`styles/<lens>.md@1`, with the Part-3 do-not matrices folded
  into each file's curation checklist), `specs/v1.json` (all 14 §5.1 assets,
  content-grounded briefs, wire blocks), `gen.py` (PayPerQ `nano-banana-2-1`,
  $15 cap enforced with an 80% stop-and-report, `client_request_id =
  pf2e-<lens>-<asset>-vN` idempotency), `compose_sheet.py` (`review.html`
  contact sheet), `promote.py` (approve/foul/run; finalizes WebP q80 at role
  caps, injects `image`/`image_alt`/`image_provenance`, refuses unapproved or
  alt-less assets). Calibration forks resolved: catholic = stained-glass
  saints; orthodox = icon-in-architecture; reformed = no divine persons +
  exemplars as narrative scenes; pentecostal = timeless-contemporary dress.
- **Code seams (Phase C):** one new IPC `get_entity_image`
  (traversal-guarded data-URL server, std-only base64, `src-tauri/src/images.rs`);
  `src/lib/images.ts` resolvers + `EntityImage.svelte` (renders nothing until
  art resolves); hero on `entity/[id]` (banner for adventure/plane, floated
  portrait otherwise); play page load-flow cover thumb + dismissible banner,
  `.handout-card` plates, scene-vista stub filled + GM "Adventure BG"
  affordance, NPC token portraits. Token art is stamped **authority-side**:
  `apply_adventure` writes the full bundled path onto each minted token
  (`Token.image`, omitted when empty) only when `images::bundled_image_exists`
  confirms the promoted portrait is embedded — the UI never re-derives paths.
  `HandoutDef`/`HandoutState` gained additive `image`/`image_alt` (handout
  bodies still render-TBD). Every seam keeps its shipped fallback.
- **Phase B (2026-10-15, done):** all 14 assets generated via PPQ
  `nano-banana-2-1` at promo pricing — **total spend $0.0000 of the $15.00
  cap** (API reported `cost_usd: 0` on every call; rows in MANIFEST.json).
  Curation read every staged plate against the lewisian do-not checklist:
  **13 approved at v1**; `handout-prayer` fouled once (cursive word-shapes —
  do-not "text/letters"), regenerated once with a letterform-free brief,
  **v2 approved**. `scene-roof-of-mercy-1` approved with note (tiny
  subordinate doorpost flame; dominant light stays the morning roof-break).
  **14/14 promoted.**
- **Bundle delta:** `lewisian/art/` = **1,021,938 B (998 KiB)** over 14 WebP
  q80 files at role caps — covers 3:4 ≤1024px (154/168 KiB), handouts ≤768px
  (71–82 KiB), tokens 256px (9–18 KiB), vistas 16:9 ≤1280px (177/196 KiB) —
  inside the ≈1 MiB v1 budget. Both adventure files carry
  `image`/`image_alt`/`image_provenance`; the 3 handout flow-items carry
  `image`/`image_alt`. Curated set (full-size cells + lineage):
  `tools/imagery/review.html`.
- **Gates after promotion:** `cargo test --all-features` green (21 suites, 0
  failed; `tests/imagery.rs` 5/5 incl. the 1 MiB ceiling and probe
  negatives), `corepack pnpm check` 0/0, `corepack pnpm test` 86/86,
  `corepack pnpm e2e` 27 passed / 4 skipped / 0 failed.
- **About-page AI-provenance copy** (§4.6(2)): landed with this change.
- **Open follow-ups:** store AI disclosure at submission; paizo-CUP read
  stays flagged for the Tier-2 §8 licensing pass.
