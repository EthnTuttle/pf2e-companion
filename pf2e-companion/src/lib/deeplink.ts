// Session invite deep-links (SHIPPING.md Tier 1 item 2).
//
// Wire format:
//   pf2e://join?url=<percent-encoded ws(s):// LAN url>[&pin=<6-char PIN>]
//   pf2e://join?ticket=<p2p ticket>
//
// PIN posture (docs/phase10/PLAN.md post-review note): "never put ?pin= in
// URLs" was about *browser history* leaks. A custom-scheme link opens the
// app — it never lands in a browser history — so `pin=` is acceptable in the
// pf2e:// scheme. The LAN/browser path (ws URL pasted into a mobile browser,
// transported by `?host=` on the served page) keeps the PIN user-typed.
//
// Everything here except `initDeepLinkHandling` is pure and unit-tested
// (deeplink.test.ts); the Tauri plugin surface is dynamic-imported so plain
// browsers, vitest, and the e2e harness (whose IPC stub rejects unknown
// commands) never touch it.

/** What a `pf2e://join…` link (or the lobby's `?join=1…` echo) decodes to. */
export interface JoinInvite {
  mode: "lan" | "ticket";
  /** ws(s):// host URL the joiner dials (LAN mode). */
  lanUrl: string | null;
  /** P2P ticket the joiner authenticates with (ticket mode). */
  ticket: string | null;
  /** 6-char table PIN, normalized uppercase; null → joiner types it. */
  pin: string | null;
}

/** The registered custom URL scheme (tauri.conf.json `plugins.deep-link`). */
export const INVITE_SCHEME = "pf2e";

// Crockford-ish table-PIN alphabet (mirrors PIN_ALPHABET in
// session/service.rs — no 0/O/1/I/L). Exactly 6 chars.
const PIN_RE = /^[23456789ABCDEFGHJKMNPQRSTVWXYZ]{6}$/;

function normalizePin(raw: string | null): string | null {
  if (raw === null) return null;
  const pin = raw.trim().toUpperCase();
  return PIN_RE.test(pin) ? pin : null;
}

function isWsUrl(raw: string): boolean {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  return (url.protocol === "ws:" || url.protocol === "wss:") && url.host !== "";
}

// iroh tickets are long base32-ish strings; we only reject obvious garbage.
// The join handler's own parsing stays the authority.
function isPlausibleTicket(raw: string): boolean {
  return raw.length >= 16 && raw.length <= 4096 && !/\s/.test(raw);
}

/**
 * Shared param parse for the `pf2e://join?…` query and the mirrored lobby
 * query. LAN wins when both are present (the LAN link is the one with a
 * PIN); a malformed PIN never sinks an otherwise valid invite — it degrades
 * to "typed by the user".
 */
export function inviteFromParams(params: URLSearchParams): JoinInvite | null {
  const lanUrl = params.get("url");
  if (lanUrl !== null && isWsUrl(lanUrl)) {
    return { mode: "lan", lanUrl, ticket: null, pin: normalizePin(params.get("pin")) };
  }
  const ticket = params.get("ticket");
  if (ticket !== null && isPlausibleTicket(ticket)) {
    // Tickets authenticate by themselves (IPC peers carry no PIN), so a
    // stray pin= is dropped rather than carried.
    return { mode: "ticket", lanUrl: null, ticket, pin: null };
  }
  return null;
}

/**
 * Parse a full deep-link URI (`pf2e://join?…`). Tolerant about the join
 * command's shape (`pf2e://join`, `pf2e://join/?…`); strict about the scheme.
 */
export function parseInviteUri(href: string): JoinInvite | null {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  if (url.protocol.toLowerCase() !== `${INVITE_SCHEME}:`) return null;
  // Non-special scheme: `pf2e://join?x` → host "join"; `pf2e:join?x` →
  // pathname "join". Accept either.
  const command =
    url.host !== "" ? url.host : url.pathname.replace(/^\/+|\/+$/g, "");
  if (command.toLowerCase() !== "join") return null;
  return inviteFromParams(url.searchParams);
}

/** Build a `pf2e://join?…` invite link. Null when there's nothing valid to share. */
export function buildInviteUri(invite: JoinInvite): string | null {
  const params = new URLSearchParams();
  if (invite.mode === "lan") {
    if (!invite.lanUrl || !isWsUrl(invite.lanUrl)) return null;
    params.set("url", invite.lanUrl);
    const pin = normalizePin(invite.pin);
    if (pin !== null) params.set("pin", pin);
  } else {
    if (!invite.ticket || !isPlausibleTicket(invite.ticket)) return null;
    params.set("ticket", invite.ticket);
  }
  return `${INVITE_SCHEME}://join?${params.toString()}`;
}

/** Query string (sans `?`) the app navigates to: `/table?join=1&…`. */
export function inviteToLobbyQuery(invite: JoinInvite): string {
  const params = new URLSearchParams();
  params.set("join", "1");
  if (invite.mode === "lan" && invite.lanUrl) params.set("url", invite.lanUrl);
  if (invite.mode === "ticket" && invite.ticket) params.set("ticket", invite.ticket);
  if (invite.pin) params.set("pin", invite.pin);
  return params.toString();
}

/**
 * Lobby side: decode `/table?join=1&…`. The `join=1` marker makes prefill
 * intentional — a stray `?url=` on the lobby URL never touches the form.
 */
export function inviteFromLobbyQuery(params: URLSearchParams): JoinInvite | null {
  if (params.get("join") !== "1") return null;
  return inviteFromParams(params);
}

/**
 * Wire the tauri-plugin-deep-link surface: resolve the cold-start URL
 * (`getCurrent`) and listen for links delivered while the app runs
 * (`onOpenUrl` — macOS/iOS/Android; on Windows/Linux a clicked link spawns
 * a new instance which re-takes the cold-start path, per the plugin docs).
 * Each parseable `pf2e://join…` URL invokes `onInvite`.
 *
 * Returns true when a cold-start invite was delivered — the caller uses it
 * to win over the first-run-wizard redirect. No-op outside Tauri; every
 * failure (browser, stubbed IPC harness) is swallowed so boot never breaks.
 */
export async function initDeepLinkHandling(
  onInvite: (invite: JoinInvite) => void,
): Promise<boolean> {
  if (typeof window === "undefined") return false;
  try {
    const { getCurrent, onOpenUrl } = await import("@tauri-apps/plugin-deep-link");
    const deliver = (urls: string[] | null): boolean => {
      let handled = false;
      for (const href of urls ?? []) {
        const invite = parseInviteUri(href);
        if (invite) {
          handled = true;
          onInvite(invite);
        }
      }
      return handled;
    };
    const coldHandled = deliver(await getCurrent());
    await onOpenUrl((urls) => {
      deliver(urls);
    });
    return coldHandled;
  } catch {
    return false;
  }
}
