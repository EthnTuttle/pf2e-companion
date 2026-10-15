// Vitest unit tests for src/lib/deeplink.ts (pure parts — the Tauri plugin
// surface in initDeepLinkHandling is desktop-shell territory and is covered
// by the Tier-1-#1 real-device evening, not unit tests).
import { describe, expect, test } from "vitest";
import {
  buildInviteUri,
  inviteFromLobbyQuery,
  inviteFromParams,
  inviteToLobbyQuery,
  parseInviteUri,
  type JoinInvite,
} from "./deeplink";

const LAN = "ws://192.168.1.42:4780/abc123";
const PIN = "A7K9M2";
const TICKET = "node1" + "k4qnae7m2paafl".repeat(8); // 128 chars, no spaces

function enc(s: string): string {
  return encodeURIComponent(s);
}

describe("parseInviteUri", () => {
  test("LAN invite with PIN", () => {
    expect(parseInviteUri(`pf2e://join?url=${enc(LAN)}&pin=${PIN}`)).toEqual({
      mode: "lan",
      lanUrl: LAN,
      ticket: null,
      pin: PIN,
    });
  });

  test("LAN invite without PIN keeps the URL (pin stays user-typed)", () => {
    expect(parseInviteUri(`pf2e://join?url=${enc(LAN)}`)).toEqual({
      mode: "lan",
      lanUrl: LAN,
      ticket: null,
      pin: null,
    });
  });

  test("P2P ticket invite", () => {
    expect(parseInviteUri(`pf2e://join?ticket=${enc(TICKET)}`)).toEqual({
      mode: "ticket",
      lanUrl: null,
      ticket: TICKET,
      pin: null,
    });
  });

  test("trailing-slash join command parses", () => {
    expect(parseInviteUri(`pf2e://join/?url=${enc(LAN)}`)?.lanUrl).toBe(LAN);
  });

  test("wrong scheme / command / garbage are rejected", () => {
    expect(parseInviteUri(`https://join?url=${enc(LAN)}`)).toBeNull();
    expect(parseInviteUri(`pf2e://host?url=${enc(LAN)}`)).toBeNull();
    expect(parseInviteUri("pf2e://join")).toBeNull();
    expect(parseInviteUri("pf2e://join?url=not-a-url")).toBeNull();
    expect(parseInviteUri("not even a url")).toBeNull();
    expect(parseInviteUri("")).toBeNull();
  });

  test("http(s) url= is rejected — LAN invites are ws(s) only", () => {
    expect(
      parseInviteUri(`pf2e://join?url=${enc("https://192.168.1.42:4780/abc123")}`),
    ).toBeNull();
  });

  test("malformed PIN degrades to null instead of sinking the invite", () => {
    const r = parseInviteUri(`pf2e://join?url=${enc(LAN)}&pin=A7K9M2XX`);
    expect(r?.pin).toBeNull();
    expect(r?.lanUrl).toBe(LAN);
    // chars outside the Crockford-ish alphabet (0, 1, I, L, O)
    expect(parseInviteUri(`pf2e://join?url=${enc(LAN)}&pin=I1LO0O`)?.pin).toBeNull();
  });

  test("lowercase PIN normalizes to uppercase", () => {
    expect(parseInviteUri(`pf2e://join?url=${enc(LAN)}&pin=a7k9m2`)?.pin).toBe(PIN);
  });

  test("pin= on a ticket invite is dropped (ticket path carries no PIN)", () => {
    const r = parseInviteUri(`pf2e://join?ticket=${enc(TICKET)}&pin=${PIN}`);
    expect(r?.mode).toBe("ticket");
    expect(r?.pin).toBeNull();
  });

  test("implausible tickets rejected", () => {
    expect(parseInviteUri("pf2e://join?ticket=short")).toBeNull();
    expect(
      parseInviteUri(`pf2e://join?ticket=${enc(TICKET.slice(0, 40) + " has spaces " + TICKET)}`),
    ).toBeNull();
  });

  test("valid LAN mode wins over a stray invalid ticket", () => {
    const r = parseInviteUri(`pf2e://join?url=${enc(LAN)}&ticket=short`);
    expect(r?.mode).toBe("lan");
    expect(r?.ticket).toBeNull();
  });
});

describe("buildInviteUri", () => {
  test("LAN + PIN round-trips through parseInviteUri", () => {
    const uri = buildInviteUri({ mode: "lan", lanUrl: LAN, ticket: null, pin: PIN });
    expect(uri).not.toBeNull();
    expect(uri!.startsWith("pf2e://join?")).toBe(true);
    expect(uri!).not.toContain(LAN); // url is percent-encoded
    expect(parseInviteUri(uri!)).toEqual({ mode: "lan", lanUrl: LAN, ticket: null, pin: PIN });
  });

  test("LAN without PIN", () => {
    const uri = buildInviteUri({ mode: "lan", lanUrl: LAN, ticket: null, pin: null });
    expect(uri).not.toBeNull();
    expect(new URL(uri!).searchParams.get("pin")).toBeNull();
  });

  test("ticket round-trip", () => {
    const uri = buildInviteUri({ mode: "ticket", lanUrl: null, ticket: TICKET, pin: null });
    expect(parseInviteUri(uri!)).toEqual({ mode: "ticket", lanUrl: null, ticket: TICKET, pin: null });
  });

  test("invalid inputs yield null", () => {
    expect(buildInviteUri({ mode: "lan", lanUrl: null, ticket: null, pin: null })).toBeNull();
    expect(buildInviteUri({ mode: "lan", lanUrl: "not-a-url", ticket: null, pin: null })).toBeNull();
    expect(buildInviteUri({ mode: "ticket", lanUrl: null, ticket: "short", pin: null })).toBeNull();
  });
});

describe("lobby query mirror", () => {
  test("round-trip via inviteToLobbyQuery / inviteFromLobbyQuery", () => {
    const invite: JoinInvite = { mode: "lan", lanUrl: LAN, ticket: null, pin: PIN };
    const query = inviteToLobbyQuery(invite);
    expect(query).toContain("join=1");
    expect(inviteFromLobbyQuery(new URLSearchParams(query))).toEqual(invite);
  });

  test("ticket-only invite lobby round-trip", () => {
    const invite: JoinInvite = { mode: "ticket", lanUrl: null, ticket: TICKET, pin: null };
    expect(inviteFromLobbyQuery(new URLSearchParams(inviteToLobbyQuery(invite)))).toEqual(invite);
  });

  test("missing join marker is ignored (stray ?url= never prefills)", () => {
    expect(inviteFromLobbyQuery(new URLSearchParams(`url=${enc(LAN)}&pin=${PIN}`))).toBeNull();
  });

  test("inviteFromParams is scheme-agnostic (shared core)", () => {
    expect(inviteFromParams(new URLSearchParams(`url=${enc(LAN)}&pin=${PIN}`))).toEqual({
      mode: "lan",
      lanUrl: LAN,
      ticket: null,
      pin: PIN,
    });
    expect(inviteFromParams(new URLSearchParams())).toBeNull();
  });
});
