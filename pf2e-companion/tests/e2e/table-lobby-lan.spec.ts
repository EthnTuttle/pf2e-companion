// F-R1 regression pins — deterministic-lane coverage of the lobby's LAN Join
// button. Why this file exists: F-R1 shipped because the deterministic lane
// stubbed `session_join_lan` (a command that never existed in src-tauri —
// the stub said it did) and the live lane bypassed the lobby via URL params.
// The stub is gone; these tests pin the real contract WITHOUT any
// session-join stub, so harness/app drift fails loudly here:
//   1. Join hands the typed credentials to /table/play as ?host=…&pin=…&name=
//      (createTransport picks wsTransport for `?host=` in EVERY shell — see
//      transport.ts) and records ZERO session-establishment IPC calls.
//   2. A non-ws(s) URL is rejected inline with a readable message — no
//      navigation, no IPC (the device report's "no raw TypeError" ask).
// The live lane's L5 drives the same button against the real Rust server.
import { expect, test } from "@playwright/test";
import { installHarness, invocations } from "./helpers/harness";

const NAME_RE = { exact: true };

test("lobby LAN Join hands off to /table/play?host=… (no IPC round-trip)", async ({
  page,
}) => {
  await installHarness(page); // tauri: true — the native-shell class
  await page.goto("/table");

  await page.getByLabel("Name", NAME_RE).fill("Alice");
  await page.getByRole("button", { name: "LAN URL", exact: true }).click();
  // Wire-form paste (`…/ws/<id>`), lowercase PIN: the lobby must normalize.
  await page
    .getByLabel("Host LAN URL", NAME_RE)
    .fill("ws://127.0.0.1:9/ws/sess-e2e/");
  await page.getByLabel("Table PIN (6 chars)", NAME_RE).fill("a7k9m2");
  await page.getByRole("button", { name: "Join Session" }).click();

  // Navigation contract: `/ws` prefix collapsed back, trailing slash
  // dropped, PIN uppercased, name carried.
  await expect(page).toHaveURL(/\/table\/play\?/);
  const params = new URL(page.url()).searchParams;
  expect(params.get("name")).toBe("Alice");
  expect(params.get("host")).toBe("ws://127.0.0.1:9/sess-e2e");
  expect(params.get("pin")).toBe("A7K9M2");

  // The harness records every invoke; only the lobby's boot-time Saved
  // Sessions refresh may appear — no host/join/load/snapshot round-trip.
  const sessionCalls = (await invocations(page))
    .map((r) => r.cmd)
    .filter((c) => c.startsWith("session_"));
  expect(sessionCalls).toEqual(["session_list_saved"]);
});

test("lobby LAN Join rejects a non-ws URL inline, before any IPC", async ({
  page,
}) => {
  await installHarness(page);
  await page.goto("/table");

  await page.getByLabel("Name", NAME_RE).fill("Alice");
  await page.getByRole("button", { name: "LAN URL", exact: true }).click();
  await page.getByLabel("Host LAN URL", NAME_RE).fill("http://127.0.0.1:4780/abc");
  await page.getByRole("button", { name: "Join Session" }).click();

  await expect(page.locator("p.err")).toContainText("must start with ws://");
  await expect(page).toHaveURL(/\/table$/);
  const sessionCalls = (await invocations(page))
    .map((r) => r.cmd)
    .filter((c) => c.startsWith("session_"));
  expect(sessionCalls).toEqual(["session_list_saved"]);
});
