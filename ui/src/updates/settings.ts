import type { Channel } from "../api";

/** Where the update choices are kept between sessions, like the language. */
export const CHANNEL_STORAGE_KEY = "fabrilo.update.channel";
export const AUTO_CHECK_STORAGE_KEY = "fabrilo.update.autoCheck";

// Storage can be missing or throw (private windows, blocked site data); a choice then lasts only
// for this session.
export function readChannel(): Channel {
  try {
    return localStorage.getItem(CHANNEL_STORAGE_KEY) === "beta" ? "beta" : "stable";
  } catch {
    return "stable";
  }
}

export function readAutoCheck(): boolean {
  try {
    return localStorage.getItem(AUTO_CHECK_STORAGE_KEY) !== "off";
  } catch {
    return true;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered; see readChannel.
  }
}

export const writeChannel = (channel: Channel) => write(CHANNEL_STORAGE_KEY, channel);
export const writeAutoCheck = (on: boolean) => write(AUTO_CHECK_STORAGE_KEY, on ? "on" : "off");
