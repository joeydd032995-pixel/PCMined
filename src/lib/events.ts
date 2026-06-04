// Typed wrappers around Tauri events emitted by the Rust core.
//
// Event channel names mirror `src-tauri/src/events.rs`. Payload shapes are kept
// in sync with the Rust `serde` structs. Subscribe via the helpers below so the
// channel-name strings live in exactly one place.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const Channels = {
  stats: "miner://stats",
  status: "miner://status",
  log: "miner://log",
  blockFound: "miner://block-found",
  difficulty: "network://difficulty",
} as const;

/** Generic typed subscription helper. */
export function on<T>(
  channel: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(channel, (event) => handler(event.payload));
}
