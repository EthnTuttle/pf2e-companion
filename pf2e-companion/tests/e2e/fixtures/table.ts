// Fixtures for the encounter-math and remaster-reference surfaces
// (PLAN §3.3, §4 S03–S05). Numbers match the real `xp_budget` table
// (PF2e Remaster GM Core p.49: party of 4 → 80 XP moderate, 160 XP extreme).
import type { AliasHit, MiracleHit, XpBudgetResult } from "$lib/ipc";

export const fixtureXpModerate: XpBudgetResult = {
  party_size: 4,
  difficulty: "moderate",
  xp_budget: 80,
  per_pc_adjust: 20,
  base_for_party_of_4: 80,
};

export const fixtureXpExtreme: XpBudgetResult = {
  party_size: 4,
  difficulty: "extreme",
  xp_budget: 160,
  per_pc_adjust: 40,
  base_for_party_of_4: 160,
};

/** Flat `creature_xp` stub: S03 asserts the budget, not the per-delta table. */
export const fixtureCreatureXp = 40;

/** S04: the canonical legacy→remaster pair from the README example. */
export const fixtureAliasHit: AliasHit = {
  legacy_name: "Magic Missile",
  remaster_name: "Force Barrage",
  category: "spell",
  notes: "Renamed in the Remaster.",
};

/** S05: the README's canonical miracle row (Mt 14:25 → Water Walk, Gospels). */
export const fixtureMiracleHit: MiracleHit = {
  miracle: "Jesus walks on the sea",
  reference: "Mt 14:25",
  book: "Gospels",
  spell_name: "Water Walk",
  tradition: "divine",
  sanctification: null,
  notes: null,
};
