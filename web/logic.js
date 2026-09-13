export const HISTORY_KEY = "findout.history.v1";
export const ACTIVATED_KEY = "findout.activated.v1";
export const DEVICE_KEY = "findout.trial-device.v1";
export const INSTALL_HINT_KEY = "findout.install-hint.v1";
export const MAX_HISTORY = 5;
export const MAX_PREVIOUS_TURNS = 4;
export const MAX_TURN_CHARS = 2000;

export function clipText(value, limit = MAX_TURN_CHARS) {
  return Array.from(String(value ?? "")).slice(0, limit).join("");
}

export function normalizeHistory(value) {
  if (!Array.isArray(value)) return [];
  return value
    .filter((turn) => turn && typeof turn.query === "string" && typeof turn.answer === "string")
    .map((turn) => ({
      query: clipText(turn.query),
      answer: clipText(turn.answer),
      searched: Boolean(turn.searched),
      timestamp: Number.isFinite(turn.timestamp) ? turn.timestamp : Date.now(),
    }))
    .slice(-MAX_HISTORY);
}

export function appendHistory(history, turn) {
  return normalizeHistory([...normalizeHistory(history), turn]);
}

export function previousTurns(history) {
  return normalizeHistory(history)
    .slice(-MAX_PREVIOUS_TURNS)
    .map(({ query, answer }) => ({ query, answer }));
}

export function makeQueryPayload({ query, history, forceSearch = false, image = null, systemContext = null }) {
  const cleanQuery = String(query ?? "").trim();
  if (!cleanQuery || Array.from(cleanQuery).length > 4000) throw new Error("Enter a question up to 4,000 characters");
  return {
    query: cleanQuery,
    previous_turns: previousTurns(history),
    force_search: Boolean(forceSearch),
    ...(image ? { image } : {}),
    ...(systemContext ? { system_context: systemContext } : {}),
  };
}

export function randomDeviceId(cryptoObject = globalThis.crypto) {
  const bytes = new Uint8Array(32);
  cryptoObject.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export const THREADS_KEY = "findout.threads.v2";
export const MAX_THREAD_TURNS = 100;
export function normalizeThreads(value) {
  if (!Array.isArray(value)) return [];
  return value.filter(t => t && typeof t.id === "string" && Array.isArray(t.turns))
    .map(t => ({ id: t.id.slice(0, 100), turns: t.turns.filter(turn => turn && typeof turn.query === "string" && typeof turn.answer === "string").slice(0, MAX_THREAD_TURNS).map(turn => ({
      query: clipText(turn.query, 4000), answer: clipText(turn.answer, 128 * 1024),
      searched: turn.searched === true, hadImage: turn.hadImage === true,
      timestamp: Number.isFinite(turn.timestamp) ? turn.timestamp : 0,
    })) })).filter(t => t.turns.length).slice(-MAX_HISTORY);
}
export function updateThread(threads, id, turns) {
  return normalizeThreads([...threads.filter(t => t.id !== id), { id, turns }]);
}
export function answerIndicators(turn) {
  return [turn.searched ? "🌐" : "", turn.hadImage ? "📷" : ""].filter(Boolean).join(" ");
}
export function threadText(turns) {
  return turns.map(turn => `${turn.query}\n${answerIndicators(turn)}${answerIndicators(turn) ? " " : ""}${turn.answer}`).join("\n\n");
}
