// Fixtures for the vault surface (PLAN §3.3, §4 S07–S09 land in 12.1).
// `fixtureCampaign` is also the single campaign in BASE_RESPONSES — the
// boot-time `list_campaigns` stub for every scenario (PLAN §3.3).
import type { Campaign, EntityDetail, EntitySummary } from "$lib/ipc";

export const fixtureCampaign: Campaign = {
  id: "millbrook",
  name: "Millbrook",
  default_lens: null,
  entity_count: 0,
};

export const fixtureEntitySummaries: EntitySummary[] = [
  {
    id: "millbrook:note-widows-lamp",
    title: "The Widow's Lamp — prep notes",
    type: "note",
    lens: null,
  },
  {
    id: "millbrook:npc-marta-veldin",
    title: "Marta Veldin",
    type: "npc",
    lens: null,
  },
];

/**
 * Full detail for the EntityEditor edit-mode scenarios (S08/S09). Same id as
 * the list row above; `source: "vault"` is what makes /vault/[id] render the
 * editor instead of the "cannot edit reference content" gate.
 */
export const fixtureVaultEntity: EntityDetail = {
  id: "millbrook:npc-marta-veldin",
  title: "Marta Veldin",
  type: "npc",
  lens: null,
  license_provenance: "homebrew",
  source: "vault",
  frontmatter: { campaign_id: "millbrook" },
  body: "# Marta Veldin\n\nProprietor of the Widow's Lamp. Knows more than she pours.\n",
  statblock: null,
};
