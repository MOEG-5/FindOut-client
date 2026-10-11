export const FOCUS_GRACE_MS = 10_000;
export const FOCUS_STORAGE_KEY = "findout.away-since.v1";

function readStoredAway(storage, key) {
  try {
    const raw = storage?.getItem(key);
    if (raw == null) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? value : null;
  } catch {
    return null;
  }
}

function rememberAway(storage, key, timestamp) {
  try {
    storage?.setItem(key, String(timestamp));
  } catch {
    /* Keep the in-memory deadline when storage is unavailable. */
  }
}

function forgetAway(storage, key) {
  try {
    storage?.removeItem(key);
  } catch {
    /* Storage being unavailable must not break focus tracking. */
  }
}

// A reload or OS discard wipes the in-memory deadline. If the page was already
// away longer than the grace period when it went away, the next load must
// still start fresh instead of resuming the old thread.
export function consumeExpiredAway({ storage = null, key = FOCUS_STORAGE_KEY, now = Date.now(), grace = FOCUS_GRACE_MS } = {}) {
  const since = readStoredAway(storage, key);
  if (since === null) return false;
  if (now - since < grace) return false;
  forgetAway(storage, key);
  return true;
}

function resolveStorage(explicit, appWindow) {
  if (explicit !== undefined) return explicit;
  try {
    return appWindow?.localStorage ?? null;
  } catch {
    return null;
  }
}

// Check elapsed time again on return: mobile browsers can suspend background timers.
// The away timestamp is also persisted so a reload or OS discard after a long
// absence still starts fresh instead of resuming the previous thread.
export function watchFocusLoss(onReset, { window: appWindow = window, document: appDocument = document, storage: explicitStorage, storageKey = FOCUS_STORAGE_KEY } = {}) {
  const storage = resolveStorage(explicitStorage, appWindow);
  let awaySince = null;
  let reset = false;
  let timer = null;

  function resetIfExpired() {
    if (awaySince !== null && !reset && Date.now() - awaySince >= FOCUS_GRACE_MS) {
      reset = true;
      onReset();
    }
  }

  function armTimer() {
    clearTimeout(timer);
    const remaining = awaySince === null ? FOCUS_GRACE_MS : Math.max(0, FOCUS_GRACE_MS - (Date.now() - awaySince));
    timer = setTimeout(resetIfExpired, remaining);
  }

  function leave() {
    if (awaySince !== null) return;
    // Keep the earliest away time across a reload while hidden.
    const stored = readStoredAway(storage, storageKey);
    awaySince = stored === null ? Date.now() : Math.min(stored, Date.now());
    rememberAway(storage, storageKey, awaySince);
    reset = false;
    armTimer();
  }

  function returnToApp() {
    if (appDocument.hidden) return;
    resetIfExpired();
    clearTimeout(timer);
    timer = null;
    awaySince = null;
    reset = false;
    forgetAway(storage, storageKey);
  }

  function visibilityChanged() {
    if (appDocument.hidden) leave();
    else returnToApp();
  }

  appWindow.addEventListener("blur", leave);
  appWindow.addEventListener("focus", returnToApp);
  appWindow.addEventListener("pageshow", returnToApp);
  appWindow.addEventListener("pagehide", leave);
  appDocument.addEventListener("visibilitychange", visibilityChanged);
  if (appDocument.hidden) leave();
  else forgetAway(storage, storageKey);

  return () => {
    clearTimeout(timer);
    appWindow.removeEventListener("blur", leave);
    appWindow.removeEventListener("focus", returnToApp);
    appWindow.removeEventListener("pageshow", returnToApp);
    appWindow.removeEventListener("pagehide", leave);
    appDocument.removeEventListener("visibilitychange", visibilityChanged);
  };
}
