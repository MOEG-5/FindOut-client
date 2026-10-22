import {
  ACTIVATED_KEY,
  DEVICE_KEY,
  HISTORY_KEY,
  INSTALL_HINT_KEY,
  THREADS_KEY, MAX_THREAD_TURNS, normalizeThreads, updateThread, sourcesLabel, threadText, installPlatform,
  makeQueryPayload,
  normalizeHistory,
  randomDeviceId,
} from "./logic.js";
import { quotaExhaustionStatus, quotaStatus } from "./quota.js";
import { consumeExpiredAway, watchFocusLoss } from "./focus.js";

const $ = (selector) => document.querySelector(selector);
const elements = {
  activationView: $("#activationView"), mainView: $("#mainView"), activationForm: $("#activationForm"),
  activationInput: $("#activationInput"), activationButton: $("#activationButton"), activationStatus: $("#activationStatus"),
  queryForm: $("#queryForm"), queryInput: $("#queryInput"), queryStatus: $("#queryStatus"), sendButton: $("#sendButton"),
  imageInput: $("#imageInput"), attachButton: $("#attachButton"),
  imagePreview: $("#imagePreview"), imageThumb: $("#imageThumb"), imageName: $("#imageName"), removeImageButton: $("#removeImageButton"),
  answerView: $("#answerView"), answerQuestion: $("#answerQuestion"), answerText: $("#answerText"), answerSource: $("#answerSource"),
  copyButton: $("#copyButton"), allowance: $("#allowance"),
  historySection: $("#historySection"), historyList: $("#historyList"), historyTemplate: $("#historyItemTemplate"),
  newButton: $("#newButton"), settingsButton: $("#settingsButton"), settingsPanel: $("#settingsPanel"), installButton: $("#installButton"),
  clearHistoryButton: $("#clearHistoryButton"), signOutButton: $("#signOutButton"),
  installHint: $("#installHint"), installHintButton: $("#installHintButton"), dismissInstallHint: $("#dismissInstallHint"),
  installDialog: $("#installDialog"),
};

let threads = loadThreads();
// A reload or OS discard after a long absence must still start a new
// conversation instead of resuming the previous thread. Completed threads
// stay in Recent; only the active context is dropped.
let startFresh = false;
try {
  startFresh = consumeExpiredAway({ storage: localStorage });
} catch {
  startFresh = false;
}
let activeThreadId = (!startFresh && threads.at(-1)?.id) || crypto.randomUUID();
let history = (!startFresh && threads.at(-1)?.turns) || [];
let attachedImage = null;
let attachedPreviewUrl = null;
let lastAnswer = null;
let deferredInstallPrompt = null;
let activeRequest = null;
const installTarget = installPlatform(navigator);
const standalone = matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;

class ApiError extends Error {
  constructor(message, status, headers) {
    super(message);
    this.status = status;
    this.headers = headers;
  }
}

function loadThreads() {
  try {
    const saved = localStorage.getItem(THREADS_KEY);
    if (saved !== null) return normalizeThreads(JSON.parse(saved));
    const legacy = normalizeHistory(JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]"));
    return legacy.length ? [{ id: crypto.randomUUID(), turns: legacy }] : [];
  } catch { return []; }
}

function saveHistory() {
  threads = updateThread(threads, activeThreadId, history);
  try {
    localStorage.setItem(THREADS_KEY, JSON.stringify(threads));
    localStorage.removeItem(HISTORY_KEY);
  } catch { elements.queryStatus.textContent = "History is available for this session but could not be saved on this device."; }
  renderHistory();
}

function newConversation(focusInput = true) {
  activeRequest?.abort();
  activeRequest = null;
  setBusy(false);
  activeThreadId = crypto.randomUUID();
  history = [];
  restoreLatestAnswer();
  elements.queryInput.value = "";
  elements.queryStatus.textContent = "";
  resizeComposer();
  clearImage();
  if (focusInput) elements.queryInput.focus();
}
// Phones reset on their own when FindOut is left; a desktop tab rarely is.
elements.newButton.addEventListener("click", () => newConversation());
watchFocusLoss(() => {
  if (!elements.mainView.hidden) newConversation(false);
});

function setActivated(value) {
  localStorage.setItem(ACTIVATED_KEY, value ? "true" : "false");
  elements.activationView.hidden = value;
  elements.mainView.hidden = !value;
  if (value) {
    renderHistory();
    restoreLatestAnswer();
  } else {
    elements.newButton.hidden = true;
    updateInstallUi();
  }
}

// Quota is only shown from the headers of a live response, never replayed.
function restoreLatestAnswer() {
  renderAnswer(history.at(-1));
  elements.allowance.textContent = "";
}

function getDeviceId() {
  let value = localStorage.getItem(DEVICE_KEY);
  if (!/^[a-f0-9]{64}$/.test(value || "")) {
    value = randomDeviceId();
    localStorage.setItem(DEVICE_KEY, value);
  }
  return value;
}

function setBusy(busy, activation = false) {
  const button = activation ? elements.activationButton : elements.sendButton;
  button.disabled = busy;
  if (activation) button.textContent = busy ? "WAIT…" : "ACTIVATE";
  else button.querySelector("span").textContent = busy ? "···" : "→";
  if (!activation) {
    elements.queryInput.disabled = busy;
    elements.attachButton.disabled = busy;
    elements.newButton.disabled = busy;
    elements.clearHistoryButton.disabled = busy;
    elements.signOutButton.disabled = busy;
    elements.historyList.inert = busy;
  }
}

async function api(path, options = {}) {
  let response;
  try {
    response = await fetch(path, {
      ...options,
      credentials: "same-origin",
      headers: { "Content-Type": "application/json", ...(options.headers || {}) },
    });
  } catch {
    throw new Error("Cannot reach the FindOut server");
  }
  let body = {};
  try { body = await response.json(); } catch { /* server returned no JSON */ }
  if (!response.ok) {
    if (response.status === 401 && path !== "/api/activate") setActivated(false);
    throw new ApiError(
      body.message || (response.status === 401 ? "Activation required" : "FindOut could not complete that request"),
      response.status,
      response.headers,
    );
  }
  return { body, headers: response.headers };
}

elements.activationForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const activationKey = elements.activationInput.value.trim();
  elements.activationStatus.textContent = "";
  setBusy(true, true);
  try {
    await api("/api/activate", {
      method: "POST",
      body: JSON.stringify({ activation_key: activationKey, ...(activationKey.toLowerCase() === "trial" ? { device_id: getDeviceId() } : {}) }),
    });
    elements.activationInput.value = "";
    setActivated(true);
    elements.queryInput.focus();
  } catch (error) {
    elements.activationStatus.textContent = error.message;
  } finally { setBusy(false, true); }
});

elements.queryForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await ask({ query: elements.queryInput.value, image: attachedImage });
});

async function ask({ query, image }) {
  if (activeRequest) return;
  const request = new AbortController();
  activeRequest = request;
  elements.queryStatus.textContent = "";
  setBusy(true);
  try {
    // A full thread stays in Recent; the next question quietly starts a new one.
    if (history.length >= MAX_THREAD_TURNS) {
      activeThreadId = crypto.randomUUID();
      history = [];
    }
    const payload = makeQueryPayload({ query, history, image, systemContext: mobileContext() });
    const { body, headers } = await api("/api/query", { method: "POST", body: JSON.stringify(payload), signal: request.signal });
    if (activeRequest !== request) return;
    if (typeof body.answer !== "string" || typeof body.searched !== "boolean") throw new Error("FindOut returned an invalid answer");
    const turn = { query: payload.query, answer: body.answer, searched: body.searched, hadImage: Boolean(payload.image), timestamp: Date.now() };
    history = [...history, turn];
    saveHistory();
    renderAnswer(turn);
    elements.allowance.textContent = quotaStatus(headers);
    elements.answerView.scrollIntoView({ behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "start" });
    elements.queryInput.value = "";
    resizeComposer();
    clearImage();
  } catch (error) {
    if (activeRequest !== request) return;
    const quotaStatusText = error instanceof ApiError && error.status === 429
      ? quotaExhaustionStatus(error.headers)
      : "";
    // The exhaustion message already carries the quota; don't repeat it below the answer.
    if (error instanceof ApiError) elements.allowance.textContent = quotaStatusText ? "" : quotaStatus(error.headers);
    elements.queryStatus.textContent = quotaStatusText || error.message;
  } finally {
    if (activeRequest === request) {
      activeRequest = null;
      setBusy(false);
    }
  }
}

// The answer area only exists once there is an answer, as on the desktop.
function renderAnswer(turn) {
  lastAnswer = turn?.answer ?? null;
  elements.answerView.hidden = !turn;
  elements.newButton.hidden = !turn;
  if (!turn) return;
  elements.answerQuestion.textContent = turn.query;
  elements.answerText.textContent = turn.answer;
  elements.answerText.classList.toggle("is-short", Array.from(turn.answer).length <= 18 && !turn.answer.includes("\n"));
  elements.answerSource.textContent = sourcesLabel(turn);
}

function renderHistory() {
  elements.historyList.replaceChildren();
  elements.historySection.hidden = threads.length === 0;
  [...threads].reverse().forEach((thread, index) => {
    const turn = thread.turns[0];
    const item = elements.historyTemplate.content.firstElementChild.cloneNode(true);
    const button = item.querySelector(".history-question");
    const answer = item.querySelector(".history-answer");
    item.querySelector(".history-index").textContent = String(index + 1).padStart(2, "0");
    item.querySelector(".history-title").textContent = turn.query;
    answer.textContent = threadText(thread.turns);
    const resume = document.createElement("button");
    resume.type = "button";
    resume.className = "text-button";
    resume.textContent = "CONTINUE THREAD";
    resume.addEventListener("click", () => {
      activeThreadId = thread.id;
      history = [...thread.turns];
      clearImage();
      restoreLatestAnswer();
      elements.queryInput.value = "";
      elements.queryInput.focus();
    });
    answer.append(document.createElement("br"), resume);
    button.addEventListener("click", () => {
      const open = button.getAttribute("aria-expanded") === "true";
      button.setAttribute("aria-expanded", String(!open));
      answer.hidden = open;
    });
    elements.historyList.append(item);
  });
  updateInstallUi();
}

elements.copyButton.addEventListener("click", async () => {
  if (!lastAnswer) return;
  try {
    await navigator.clipboard.writeText(lastAnswer);
    elements.copyButton.textContent = "COPIED";
    setTimeout(() => { elements.copyButton.textContent = "COPY"; }, 1600);
  } catch { elements.queryStatus.textContent = "Could not copy the answer"; }
});

elements.attachButton.addEventListener("click", () => elements.imageInput.click());
elements.imageInput.addEventListener("change", async () => {
  const file = elements.imageInput.files?.[0];
  if (!file) return;
  const threadId = activeThreadId;
  elements.queryStatus.textContent = "Preparing image…";
  try {
    const image = await normalizeImage(file);
    if (threadId !== activeThreadId) return;
    attachedImage = image;
    if (attachedPreviewUrl) URL.revokeObjectURL(attachedPreviewUrl);
    attachedPreviewUrl = URL.createObjectURL(file);
    elements.imageThumb.src = attachedPreviewUrl;
    elements.imageName.textContent = file.name;
    elements.imagePreview.hidden = false;
    elements.queryStatus.textContent = "";
  } catch (error) {
    if (threadId !== activeThreadId) return;
    elements.queryStatus.textContent = error.message;
    clearImage();
  }
});
elements.removeImageButton.addEventListener("click", clearImage);

function clearImage() {
  attachedImage = null;
  elements.imageInput.value = "";
  elements.imagePreview.hidden = true;
  if (attachedPreviewUrl) URL.revokeObjectURL(attachedPreviewUrl);
  attachedPreviewUrl = null;
  elements.imageThumb.removeAttribute("src");
}

async function normalizeImage(file) {
  if (!/^image\/(png|jpeg)$/.test(file.type)) throw new Error("Choose a PNG or JPEG image");
  const bitmap = await createImageBitmap(file);
  if (!bitmap.width || !bitmap.height || bitmap.width > 16384 || bitmap.height > 16384 || bitmap.width * bitmap.height > 64000000) {
    bitmap.close();
    throw new Error("That image is too large to process safely");
  }
  const scale = Math.min(1, 4096 / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));
  const pad = width < 1920 && height < 1080;
  const canvasWidth = pad ? 1920 : width;
  const canvasHeight = pad ? 1080 : height;
  const canvas = document.createElement("canvas");
  canvas.width = canvasWidth;
  canvas.height = canvasHeight;
  const context = canvas.getContext("2d", { alpha: false });
  context.fillStyle = "#0e1118";
  context.fillRect(0, 0, canvasWidth, canvasHeight);
  context.drawImage(bitmap, Math.floor((canvasWidth - width) / 2), Math.floor((canvasHeight - height) / 2), width, height);
  bitmap.close();
  let blob = await canvasBlob(canvas, "image/png");
  let mimeType = "image/png";
  if (blob.size > 3000000) {
    mimeType = "image/jpeg";
    for (const quality of [0.9, 0.82, 0.74, 0.66]) {
      blob = await canvasBlob(canvas, mimeType, quality);
      if (blob.size <= 3000000) break;
    }
  }
  if (blob.size > 3000000) throw new Error("The prepared image is still above the 3 MB limit");
  return { mime_type: mimeType, data: await blobBase64(blob) };
}

function canvasBlob(canvas, type, quality) {
  return new Promise((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("Could not prepare that image")), type, quality));
}

function blobBase64(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1]);
    reader.onerror = () => reject(new Error("Could not read that image"));
    reader.readAsDataURL(blob);
  });
}

function mobileContext() {
  const platform = navigator.userAgentData?.platform || navigator.platform || "Mobile";
  return `${platform} | mobile web app`;
}

elements.queryInput.addEventListener("input", resizeComposer);
elements.queryInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    elements.queryForm.requestSubmit();
  }
});
function resizeComposer() {
  elements.queryInput.style.height = "auto";
  elements.queryInput.style.height = `${Math.min(elements.queryInput.scrollHeight, 145)}px`;
}

elements.settingsButton.addEventListener("click", () => {
  elements.settingsPanel.hidden = !elements.settingsPanel.hidden;
  elements.settingsButton.setAttribute("aria-expanded", String(!elements.settingsPanel.hidden));
});
function closeSettings() {
  elements.settingsPanel.hidden = true;
  elements.settingsButton.setAttribute("aria-expanded", "false");
}
document.addEventListener("click", (event) => {
  if (!elements.settingsPanel.hidden && !elements.settingsPanel.contains(event.target) && !elements.settingsButton.contains(event.target)) closeSettings();
});

elements.clearHistoryButton.addEventListener("click", () => {
  history = [];
  threads = [];
  activeThreadId = crypto.randomUUID();
  saveHistory();
  restoreLatestAnswer();
  closeSettings();
});

elements.signOutButton.addEventListener("click", async () => {
  try { await api("/api/logout", { method: "POST", body: "{}" }); } catch { /* local reset still applies */ }
  localStorage.removeItem(ACTIVATED_KEY);
  setActivated(false);
  closeSettings();
});

// Phones get a home-screen hint after their first answer. Nothing is offered
// inside the installed app, or once the browser reports an install.
function installState() {
  try { return localStorage.getItem(INSTALL_HINT_KEY); } catch { return null; }
}
function setInstallState(value) {
  try { localStorage.setItem(INSTALL_HINT_KEY, value); } catch { /* Hide for this session only. */ }
}
function updateInstallUi() {
  const offered = !standalone && installState() !== "installed" && Boolean(installTarget || deferredInstallPrompt);
  elements.installButton.hidden = !offered;
  elements.installHint.hidden = !offered || !installTarget || installState() === "dismissed" || elements.mainView.hidden || threads.length === 0;
  elements.installHintButton.textContent = deferredInstallPrompt ? "INSTALL" : "HOW";
}
async function offerInstall() {
  closeSettings();
  if (deferredInstallPrompt) {
    const prompt = deferredInstallPrompt;
    deferredInstallPrompt = null;
    try { await prompt.prompt(); } catch { /* The browser declined to show its prompt. */ }
    updateInstallUi();
    return;
  }
  $("#installStepsIos").hidden = installTarget !== "ios";
  $("#installStepsAndroid").hidden = installTarget !== "android";
  elements.installDialog.showModal();
}
window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  deferredInstallPrompt = event;
  // Browsers only offer this while FindOut is not installed, so a recorded
  // install was removed. Keep the menu entry, but don't repeat the hint.
  if (installState() === "installed") setInstallState("dismissed");
  updateInstallUi();
});
window.addEventListener("appinstalled", () => {
  deferredInstallPrompt = null;
  setInstallState("installed");
  updateInstallUi();
});
elements.installButton.addEventListener("click", offerInstall);
elements.installHintButton.addEventListener("click", offerInstall);
elements.dismissInstallHint.addEventListener("click", () => {
  setInstallState("dismissed");
  updateInstallUi();
});
$("#installDone").addEventListener("click", () => elements.installDialog.close());

if ("serviceWorker" in navigator) window.addEventListener("load", () => navigator.serviceWorker.register("/sw.js"));
setActivated(localStorage.getItem(ACTIVATED_KEY) === "true");
renderHistory();

const feedbackDialog = $("#feedbackDialog");
const feedbackMessage = $("#feedbackMessage");
const feedbackEmail = $("#feedbackEmail");
const feedbackLicense = $("#feedbackLicense");
const feedbackConsent = $("#feedbackConsent");
$("#feedbackButton").addEventListener("click", () => {
  closeSettings();
  feedbackConsent.checked = false;
  $("#feedbackStatus").textContent = "";
  feedbackDialog.showModal();
});
$("#feedbackClose").addEventListener("click", () => feedbackDialog.close());
for (const field of [feedbackMessage, feedbackEmail, feedbackLicense]) {
  field.addEventListener("input", () => { feedbackConsent.checked = false; });
  field.addEventListener("change", () => { feedbackConsent.checked = false; });
}
$("#feedbackSave").addEventListener("click", () => {
  const text = [
    feedbackMessage.value,
    `Reply email: ${feedbackEmail.value.trim() || "Not provided"}`,
    "Client: web/0.1.5",
    `Share license and installation IDs: ${feedbackLicense.checked ? "Yes" : "No"}`,
  ].join("\n\n");
  const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a"); link.href = url; link.download = "findout-feedback.txt"; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
$("#feedbackForm").addEventListener("submit", async event => {
  event.preventDefault();
  if ($("#feedbackSend").disabled) return;
  if (!feedbackConsent.checked) {
    $("#feedbackStatus").textContent = "Confirm the listed payload before sending.";
    return;
  }
  $("#feedbackSend").disabled = true;
  for (const id of ["#feedbackMessage", "#feedbackEmail", "#feedbackLicense", "#feedbackConsent"]) $(id).disabled = true;
  $("#feedbackStatus").textContent = "Sending…";
  try {
    await api("/api/feedback", { method: "POST", signal: AbortSignal.timeout(15000), body: JSON.stringify({
      message: feedbackMessage.value, email: feedbackEmail.value.trim(),
      include_license: feedbackLicense.checked, client: "web/0.1.5", consent: true,
    }) });
    feedbackMessage.value = "";
    feedbackConsent.checked = false;
    $("#feedbackStatus").textContent = "Feedback sent. Thank you.";
  } catch (error) { $("#feedbackStatus").textContent = `${error.message}. Your text is kept here; you can also save it.`; }
  finally { $("#feedbackSend").disabled = false; for (const id of ["#feedbackMessage", "#feedbackEmail", "#feedbackLicense", "#feedbackConsent"]) $(id).disabled = false; }
});
