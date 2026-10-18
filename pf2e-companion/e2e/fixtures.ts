// e2e/fixtures.ts — typed IPC fixture payloads for the screenshot harness.
//
// Shapes are imported from src/lib/ipc so drift from the real bindings is a
// type error (the point of typing the harness). The runner injects exactly one
// variant map per matrix row via a fixture-injector init script; the stub
// (tauri-stub.js) resolves commands against it and structuredClone's every
// result so page mutations can't leak between shots.
import type {
  AliasHit,
  Campaign,
  ChatEntry,
  CrudResult,
  DiagnosticsInfo,
  EntityDetail,
  EntitySummary,
  EvalPrompt,
  ImportReport,
  LensManifest,
  LlmStatus,
  MiracleHit,
  RagEmbedReport,
  RagIndexStats,
  RelationRow,
  RollRecord,
  SavedSessionSummary,
  SearchHit,
  SessionInfo,
  SessionSnapshot,
  SuiteSummary,
  ValidationResult,
  WizardCompleteResult,
  WizardStatus,
  XpBudgetResult,
} from "../src/lib/ipc";

/** All timestamps fixed to one epoch — determinism. */
export const T0 = 1_760_000_000_000;

/** Command → payload map. Arg-sensitive entries use `{ __by: "<arg>" }`. */
export type FixtureMap = Record<string, unknown>;

export const LENSES: LensManifest[] = [
  {
    id: "lewisian",
    label: "Lewisian (mere Christianity)",
    description:
      "Default. Charism+Lewisian magic-theology hybrid; deferred denominational specifics.",
  },
  {
    id: "catholic",
    label: "Catholic",
    description:
      "73-book canon (deuterocanon); 9-choir angels; saint-attached Champion patrons; Thaumaturge implements as relics.",
  },
  {
    id: "reformed",
    label: "Reformed",
    description:
      "66-book canon; cessationist tilt; covenant theology scaffold; Word-as-sword.",
  },
  {
    id: "pentecostal",
    label: "Pentecostal",
    description:
      "66-book canon; spiritual-warfare frame; healing/tongues/prophecy charisms.",
  },
  {
    id: "orthodox",
    label: "Orthodox",
    description:
      "LXX+ canon; theosis; icons; Aerial Toll Houses as a level-15+ post-mortem dungeon.",
  },
];

const SEARCH_HITS: SearchHit[] = [
  {
    id: "lewisian:deity/the-lord-of-hosts",
    title: "The Lord of Hosts",
    type: "deity",
    snippet:
      "Sabaoth — commander of the angelic <mark>Hosts</mark>. Under every lens the armies of heaven…",
    score: 0.912,
    source: "both",
  },
  {
    id: "lewisian:spell/hosts-of-the-airy-court",
    title: "Hosts of the Airy Court",
    type: "spell",
    snippet:
      "Summons a choir of lesser spirits; the Lewisian reading stresses <mark>hierarchy</mark>, not coercion…",
    score: 0.841,
    source: "vec",
  },
  {
    id: "lewisian:creature/aboleth-tidebreaker",
    title: "Aboleth Tidebreaker",
    type: "creature",
    snippet:
      "A memory-hoarding horror of the deep; its enslavement aura makes <mark>Hosts</mark> of its thralls…",
    score: 0.777,
    source: "fts",
  },
  {
    id: "lewisian:note/on-angelic-orders",
    title: "On Angelic Orders",
    type: "note",
    snippet:
      "Working note: the nine choirs map cleanly onto the outsider ranks; the <mark>Hosts</mark> are the rank-and-file…",
    score: 0.655,
    source: "both",
  },
];

const ENTITY_LIST: EntitySummary[] = [
  { id: "c-runes:creature/aboleth-tidebreaker", title: "Aboleth Tidebreaker", type: "creature", lens: "lewisian" },
  { id: "c-runes:npc/nualia-of-the-tide", title: "Nualia of the Tide", type: "npc", lens: "lewisian" },
  { id: "c-runes:location/the-drowned-vault", title: "The Drowned Vault", type: "location", lens: null },
  { id: "c-runes:item/widows-lamp", title: "The Widow's Lamp", type: "item", lens: "lewisian" },
  { id: "c-runes:deity/the-lord-of-hosts", title: "The Lord of Hosts", type: "deity", lens: "lewisian" },
  { id: "c-runes:spell/hosts-of-the-airy-court", title: "Hosts of the Airy Court", type: "spell", lens: null },
  { id: "c-runes:npc/father-vestibule", title: "Father Vestibule", type: "npc", lens: "catholic" },
  { id: "c-homebrew:creature/the-tar-priest", title: "The Tar Priest", type: "creature", lens: "reformed" },
];

const ABOLETH: EntityDetail = {
  id: "c-runes:creature/aboleth-tidebreaker",
  title: "Aboleth Tidebreaker",
  type: "creature",
  lens: "lewisian",
  license_provenance: "homebrew",
  source: "vault",
  frontmatter: {
    campaign_id: "c-runes",
    status: "active",
    sources: ["wiki: pf2e-biblical-reskin/aboleth", "1 Enoch 8:3"],
  },
  body: [
    "## Summary",
    "A memory-hoarding horror dragged up from under the drowned vault. It remembers every name it has ever eaten, and it is very old.",
    "",
    "## Tactics",
    "Opens with its enslavement aura on the first hero to enter the water, then retreats into [[c-runes:location/the-drowned-vault|the flooded galleries]] while its thralls hold the stairs.",
    "",
    "## Hooks",
    "- The aboleth remembers the lamp's original owner (see [[c-runes:item/widows-lamp]]).",
    "- A cure requires consecrated water — the Lewisian reading ties this to baptism imagery.",
  ].join("\n"),
  statblock: {
    name: "Aboleth",
    title: "Tidebreaker of the Drowned Vault",
    level: 5,
    edicts: ["hoard the memories of the deep", "lure the curious below"],
    anathema: ["forsake the memory of the first darkness"],
    domains: ["water", "knowledge"],
  },
};

const DEITY: EntityDetail = {
  id: "lewisian:deity/the-lord-of-hosts",
  title: "The Lord of Hosts",
  type: "deity",
  lens: "lewisian",
  license_provenance: "orc",
  source: "bundled",
  frontmatter: {
    sources: ["wiki: pf2e-biblical-reskin/sabaoth", "1 Sam 17:45", "Ps 24:10"],
  },
  body: null,
  statblock: null,
};

const RELATIONS: RelationRow[] = [
  { from_id: "c-runes:creature/aboleth-tidebreaker", edge_type: "located_in", to_id: "c-runes:location/the-drowned-vault" },
  { from_id: "c-runes:npc/nualia-of-the-tide", edge_type: "member_of", to_id: "c-runes:creature/aboleth-tidebreaker" },
];

// XP tables copied from src-tauri/src/rules.rs so the encounter page renders
// the genuine PF2e numbers.
const XP_BY_DIFFICULTY: Record<string, XpBudgetResult> = {
  trivial: { party_size: 4, difficulty: "trivial", xp_budget: 40, per_pc_adjust: 10, base_for_party_of_4: 40 },
  low: { party_size: 4, difficulty: "low", xp_budget: 60, per_pc_adjust: 20, base_for_party_of_4: 60 },
  moderate: { party_size: 4, difficulty: "moderate", xp_budget: 80, per_pc_adjust: 20, base_for_party_of_4: 80 },
  severe: { party_size: 4, difficulty: "severe", xp_budget: 120, per_pc_adjust: 30, base_for_party_of_4: 120 },
  extreme: { party_size: 4, difficulty: "extreme", xp_budget: 160, per_pc_adjust: 40, base_for_party_of_4: 160 },
};

const CREATURE_XP_BY_DELTA: Record<string, number> = {
  "-4": 10, "-3": 15, "-2": 20, "-1": 30, "0": 40,
  "1": 60, "2": 80, "3": 120, "4": 160,
};

const GM_INFO: SessionInfo = {
  session_id: "e2e-session",
  peer_id: "e2e-peer-gm-0001",
  role: "gm",
  lan_url: "http://192.168.1.42:4317/e2e-session",
  p2p_ticket: "e2e-ticket-base32-aaaaaaaa",
  name: "GM",
  pin: "1234",
};

const PLAYER_INFO: SessionInfo = {
  ...GM_INFO,
  peer_id: "e2e-peer-p2-0002",
  role: "player",
  name: "Rook",
  p2p_ticket: null,
  lan_url: null,
};

/** Full session snapshot for /table/play, delivered through the real
 * `listen` → `applyPatch` path via `__e2eEmit("session:patch", …)`. */
export const SNAPSHOT: SessionSnapshot = {
  meta: {
    id: "e2e-session",
    name: "The Widow's Lamp — Session 1",
    gm_id: "e2e-peer-gm-0001",
    created_at: T0,
    adventure_slug: "widows-lamp",
    ended: false,
  },
  players: {
    "e2e-peer-gm-0001": { name: "GM", character: null, color: "#a855f7", connected: true },
    "e2e-peer-p2-0002": {
      name: "Rook",
      character: {
        level: 1,
        class: "Fighter",
        hp: 14,
        max_hp: 18,
        ac: 18,
        conditions: ["frightened 1"],
        notes: "Wields the widow's lamp as a torch.",
      },
      color: "#3b82f6",
      connected: true,
    },
    "e2e-peer-p3-0003": {
      name: "Seraph",
      character: {
        level: 1,
        class: "Cleric",
        hp: 9,
        max_hp: 9,
        ac: 15,
        conditions: [],
        notes: "",
      },
      color: "#ef4444",
      connected: false,
    },
  },
  chat: [
    { ts: T0, from: "GM", kind: "system", text: "Session resumed from the drowned vault landing.", roll: null },
    { ts: T0 + 1000, from: "Rook", kind: "chat", text: "I check the stairs for tracks.", roll: null },
    { ts: T0 + 2000, from: "GM", kind: "chat", text: "Roll perception.", roll: null },
    {
      ts: T0 + 3000,
      from: "Rook",
      kind: "roll",
      text: "perception",
      roll: { expr: "4d6k3", total: 12, rolls: [6, 2, 4, 1], kept: [6, 4, 2] } satisfies RollRecord,
    },
    { ts: T0 + 4000, from: "Seraph", kind: "chat", text: "I light the lamp and hold it over the water.", roll: null },
    { ts: T0 + 5000, from: "GM", kind: "chat", text: "Something vast turns over beneath the surface…", roll: null },
  ] satisfies ChatEntry[],
  init: { order: ["e2e-peer-p2-0002", "e2e-peer-gm-0001", "e2e-peer-p3-0003"], active_idx: 1, round: 2 },
  scene: {
    grid: { w: 12, h: 8, size: 32 },
    background_entity_id: null,
    tokens: {
      "tok-aboleth": { token_id: "tok-aboleth", owner: "e2e-peer-gm-0001", entity_id: "c-runes:creature/aboleth-tidebreaker", pc: null, x: 6, y: 5, label: "Aboleth" },
      "tok-rook": { token_id: "tok-rook", owner: "e2e-peer-p2-0002", entity_id: null, pc: "Rook", x: 3, y: 2, label: "Rook" },
      "tok-seraph": { token_id: "tok-seraph", owner: "e2e-peer-p3-0003", entity_id: null, pc: "Seraph", x: 4, y: 2, label: "Seraph" },
    },
  },
  handouts: {
    "h-tide": { entity_id: "c-runes:location/the-drowned-vault", visible_to: "all" },
    "h-gm-tide-map": { entity_id: "c-runes:location/the-drowned-vault", visible_to: "gm" },
  },
};

const DIAGNOSTICS: DiagnosticsInfo = {
  app_version: "0.1.0",
  os: "linux",
  arch: "x86_64",
  session_id: null,
  lines: [
    "12:00:00 INFO  app start v0.1.0 (linux/x86_64)",
    "12:00:01 INFO  vault root: /home/player/.local/share/pf2e-companion/vault",
    "12:00:01 INFO  db schema v3 migrated",
    "12:00:02 INFO  rag index ok: 12 entities, 41 chunks",
    "12:00:03 WARN  llm not configured — chat disabled",
    "12:00:04 INFO  session registry idle",
  ],
};

/** The default (friends-beta) state: wizard done, LLM off, everything else
 * populated. wizard_status.needed === false is load-bearing — see the runner's
 * startup assertion. */
export const base: FixtureMap = {
  schema_version: 3,
  search: SEARCH_HITS,
  list_lenses: LENSES,
  list_campaigns: [
    { id: "c-runes", name: "Rise of the Runelords", default_lens: "lewisian", entity_count: 12 },
    { id: "c-homebrew", name: "Tidebreakers (homebrew)", default_lens: "reformed", entity_count: 4 },
  ] satisfies Campaign[],
  lookup_alias: [
    { legacy_name: "Magic Missile", remaster_name: "Force Barrage", category: "spell", notes: "Remaster rename" },
    { legacy_name: "Aasimar", remaster_name: "Aiuvarin", category: "ancestry", notes: null },
  ] satisfies AliasHit[],
  lookup_miracle: [
    {
      miracle: "Jesus walks on the water",
      reference: "Mt 14:25",
      book: "Matthew",
      spell_name: "Water Walk",
      tradition: "divine",
      sanctification: "holy",
      notes: "Remaster ritual framing; the Lewisian lens reads this as mastery over chaos.",
    },
    {
      miracle: "Elijah calls fire from heaven",
      reference: "1 Kgs 18:38",
      book: "1 Kings",
      spell_name: "Flaming Sphere",
      tradition: "divine",
      sanctification: "holy",
      notes: null,
    },
  ] satisfies MiracleHit[],
  xp_budget: { __by: "difficulty", ...XP_BY_DIFFICULTY },
  creature_xp: { __by: "partyLevelDelta", ...CREATURE_XP_BY_DELTA },
  list_entities: ENTITY_LIST,
  get_entity: { __by: "id", "c-runes:creature/aboleth-tidebreaker": ABOLETH, "lewisian:deity/the-lord-of-hosts": DEITY },
  list_relations: RELATIONS,
  llm_status: { configured: false, provider: null, model: null, key_present: false } satisfies LlmStatus,
  rag_index_stats: { indexed: true, entities: 12, chunks: 41, provider: "ollama", model: "nomic-embed-text" } satisfies RagIndexStats,
  eval_load_suite: [
    {
      id: "miracle-map-basic",
      description: "Biblical miracle → spell mapping holds",
      prompt: "What spell models Jesus walking on water?",
      lens: "lewisian",
      system_extra: null,
      expectations: [
        { kind: "tool_called", name: "lookup_miracle" },
        { kind: "text_contains", needle: "Water Walk" },
      ],
    },
    {
      id: "encounter-math-moderate",
      description: "Encounter budget math for a party of four",
      prompt: "Build a moderate encounter for a party of 4 at level 5.",
      lens: null,
      system_extra: null,
      expectations: [
        { kind: "tool_called_with", name: "xp_budget", input_contains: { difficulty: "moderate" } },
        { kind: "text_excludes", needle: "trivial" },
        { kind: "iterations_at_most", max: 3 },
      ],
    },
  ] satisfies EvalPrompt[],
  session_list_saved: [
    {
      session_id: "e2e-session",
      campaign: "c-runes",
      name: "The Widow's Lamp — Session 1",
      created_at: T0,
      adventure_slug: "widows-lamp",
      ended: false,
      saved_at: T0 + 60000,
    },
    {
      session_id: "e2e-session-old",
      campaign: "c-runes",
      name: "Sandpoint Fog",
      created_at: T0 - 86400000,
      adventure_slug: null,
      ended: true,
      saved_at: T0 - 82800000,
    },
  ] satisfies SavedSessionSummary[],
  session_host: GM_INFO,
  session_load: GM_INFO,
  session_join_peer: PLAYER_INFO,
  // F-R1: NO session_join_lan stub — the command does not exist in src-tauri;
  // the lobby's LAN join is pure wsTransport (`?host=` handoff), so any IPC
  // round-trip from it must throw the loud unknown-command error here.
  session_intent: null,
  session_leave: null,
  session_export_log: "/home/player/Downloads/aboleth-tide-2025-10-09.log.md",
  diagnostics_get: DIAGNOSTICS,
  "plugin:dialog|save": "/home/player/Downloads/aboleth-tide-2025-10-09.log.md",
  "plugin:dialog|open": null,
  wizard_status: {
    needed: false,
    default_vault: "/home/player/.local/share/pf2e-companion/vault",
  } satisfies WizardStatus,
  wizard_complete: { restart_required: false } satisfies WizardCompleteResult,
  app_restart: null,
  // Mutations + rarely-hit commands: minimal typed stubs so interactive
  // exploration (--debug) never trips the loud unknown-command throw.
  create_campaign: { id: "c-new", name: "New campaign", default_lens: null, entity_count: 0 } satisfies Campaign,
  create_entity: { id: "c-runes:note/new-note", file_path: "/vault/c-runes/note/new-note.md" } satisfies CrudResult,
  update_entity: { id: "c-runes:creature/aboleth-tidebreaker", file_path: "/vault/c-runes/creature/aboleth-tidebreaker.md" } satisfies CrudResult,
  delete_entity: null,
  add_relation: null,
  delete_relation: null,
  llm_configure: { configured: true, provider: "ollama", model: "qwen3:4b", key_present: false } satisfies LlmStatus,
  llm_clear_config: { configured: false, provider: null, model: null, key_present: false } satisfies LlmStatus,
  rag_reindex: { provider: "ollama", model: "nomic-embed-text", entities_processed: 12, chunks_embedded: 41 } satisfies RagEmbedReport,
  eval_run: { total: 0, passed: 0, failed: 0, results: [] } satisfies SuiteSummary,
  import_foundry_pack: { files_seen: 0, files_imported: 0, files_skipped: 0, errors: [] } satisfies ImportReport,
  validate_statblock: { valid: true, errors: [], warnings: [] } satisfies ValidationResult,
};

export const variants: Record<"default" | "wizardFresh" | "chatConfigured", FixtureMap> = {
  default: base,
  // Spread override on default — never standalone (§5 hazard: needed: true
  // redirects every non-onboarding route to /onboarding).
  wizardFresh: {
    ...base,
    wizard_status: {
      needed: true,
      default_vault: "/home/player/.local/share/pf2e-companion/vault",
    } satisfies WizardStatus,
  },
  // The chat page gates sending on llmState.status?.configured.
  chatConfigured: {
    ...base,
    llm_status: { configured: true, provider: "ollama", model: "qwen3:4b", key_present: false } satisfies LlmStatus,
  },
};
