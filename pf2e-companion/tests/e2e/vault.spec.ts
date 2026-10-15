// Group B — vault (PLAN §4, S07–S09).
// Every IPC call these routes make is stubbed via installHarness; assertions
// target the real rendered DOM of src/routes/vault + src/lib/EntityEditor.
//
// §4→reality adaptations (recorded in PLAN §9, 12.1 note):
// - S08: the editor never calls validate_statblock (that tool exists only for
//   the LLM agent loop). A blank title never even submits (native `required`
//   attribute); a whitespace-only title hits the client-side gate in
//   handleSave — zero IPC. The server-side failure surface is exercised by
//   stubbing update_entity → {error}. Both legs are asserted, in that order.
// - S09: on save success EntityEditor navigates client-side to /entity/<id>,
//   which re-invokes get_entity — stubbed as a two-entry queue. The PLAN's
//   "list_entities re-stubbed post-save" never happens (no return to /vault).
import { expect, test } from "@playwright/test";
import { installHarness, invocations, shot } from "./helpers/harness";
import { fixtureEntitySummaries, fixtureVaultEntity } from "./fixtures/vault";

const vaultEditUrl = `/vault/${encodeURIComponent(fixtureVaultEntity.id)}`;
const entityViewUrl = `/entity/${encodeURIComponent(fixtureVaultEntity.id)}`;

test("S07 vault-list", async ({ page }) => {
  await installHarness(page, {
    list_entities: { ok: fixtureEntitySummaries },
  });
  await page.goto("/vault");

  await expect(
    page.getByRole("heading", { level: 1, name: "Vault" }),
  ).toBeVisible();
  // Campaign selection: the layout picker and the vault page's campaign list
  // both settle on the (only) fixture campaign "Millbrook".
  await expect(page.getByLabel("Active campaign")).toHaveValue("millbrook");
  await expect(page.locator(".camp.active .name")).toHaveText("Millbrook");
  await expect(page.locator(".entities h2")).toHaveText(
    "Entities · millbrook",
  );

  // Both fixture entities render as clickable rows with type chips.
  const rows = page.locator(".entities .hits li:not(.empty)");
  await expect(rows).toHaveCount(fixtureEntitySummaries.length);
  for (const e of fixtureEntitySummaries) {
    const row = page.locator(".entities .hits a.hit", { hasText: e.title });
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute(
      "href",
      `/vault/${encodeURIComponent(e.id)}`,
    );
    await expect(row.locator(".type")).toHaveText(e.type);
  }

  await shot(page, "vault/01-list");
});

test("S08 vault-editor-validation", async ({ page }) => {
  await installHarness(page, {
    get_entity: { ok: fixtureVaultEntity },
    list_relations: { ok: [] },
    update_entity: { error: "validation failed: name required" },
  });
  await page.goto(vaultEditUrl);

  // Editor boots in edit mode with the entity's fields loaded.
  await expect(
    page.getByRole("heading", { level: 1, name: fixtureVaultEntity.title }),
  ).toBeVisible();
  const titleInput = page.locator("#title-input");
  await expect(titleInput).toHaveValue(fixtureVaultEntity.title);

  // Leg 1 — client-side gate. A whitespace-only title passes the native
  // `required` attribute but handleSave rejects it before any IPC runs.
  // (A fully empty title would never even reach handleSave: native constraint
  // validation swallows the submit.)
  await titleInput.fill(" ");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText("title is required");
  expect(
    (await invocations(page)).filter((r) => r.cmd === "update_entity"),
  ).toHaveLength(0);

  // Leg 2 — server-side rejection: a real edit passes the client gate, the
  // stubbed update_entity throws, and the error string surfaces inline.
  await titleInput.fill("Marta Veldin (revised)");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "validation failed: name required",
  );
  await shot(page, "vault/02-editor-validation");

  const updates = (await invocations(page)).filter(
    (r) => r.cmd === "update_entity",
  );
  expect(updates).toHaveLength(1);
  const args = updates[0]?.args as { id: string; patch: { title?: string } };
  expect(args.id).toBe(fixtureVaultEntity.id);
  expect(args.patch.title).toBe("Marta Veldin (revised)");
  // The editor save flow never invokes validate_statblock (PLAN §4 S08
  // imagined it might; in reality that command is an LLM-agent tool only).
  expect(
    (await invocations(page)).filter((r) => r.cmd === "validate_statblock"),
  ).toHaveLength(0);
});

test("S09 vault-editor-save", async ({ page }) => {
  const renamed = "Marta Veldin (revised)";
  await installHarness(page, {
    // First read: the editor's load. Second (queue semantics): after a
    // successful save EntityEditor navigates client-side to /entity/<id>,
    // which re-reads the entity and must render the renamed title.
    get_entity: {
      queue: [
        { ok: fixtureVaultEntity },
        { ok: { ...fixtureVaultEntity, title: renamed } },
      ],
    },
    list_relations: { ok: [] },
    update_entity: {
      ok: {
        id: fixtureVaultEntity.id,
        file_path: "vault/millbrook/npc/marta-veldin.md",
      },
    },
  });
  await page.goto(vaultEditUrl);
  const titleInput = page.locator("#title-input");
  await expect(titleInput).toHaveValue(fixtureVaultEntity.title);

  await titleInput.fill(renamed);
  await page.getByRole("button", { name: "Save", exact: true }).click();

  // Save success surface: client-side navigation to the entity detail route,
  // renamed title rendered, no error banner anywhere on either surface.
  await expect(
    page.getByRole("heading", { level: 1, name: renamed }),
  ).toBeVisible();
  expect(page.url()).toContain(entityViewUrl);
  await expect(page.locator(".err")).toHaveCount(0);
  await shot(page, "vault/03-editor-saved");

  // Exactly one update_entity with the full emitted patch shape.
  const updates = (await invocations(page)).filter(
    (r) => r.cmd === "update_entity",
  );
  expect(updates).toHaveLength(1);
  expect(updates[0]?.args).toEqual({
    id: fixtureVaultEntity.id,
    patch: {
      title: renamed,
      // True behavior: the editor's lens select initializes to the active
      // default lens, so saving a lens-less entity re-tags it "lewisian".
      lens: "lewisian",
      license_provenance: null,
      body: fixtureVaultEntity.body,
      frontmatter: {},
    },
  });
});
