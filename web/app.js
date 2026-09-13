import {
  ACTIVATED_KEY,
  DEVICE_KEY,
  HISTORY_KEY,
  INSTALL_HINT_KEY,
  THREADS_KEY, MAX_THREAD_TURNS, normalizeThreads, updateThread, answerIndicators, threadText,
  makeQueryPayload,
  normalizeHistory,
  randomDeviceId,
} from "./logic.js";

const $ = (selector) => document.querySelector(selector);
const elements = {
  activationView: $("#activationView"), mainView: $("#mainView"), activationForm: $("#activationForm"),
  activationInput: $("#activationInput"), activationButton: $("#activationButton"), activationStatus: $("#activationStatus"),
  queryForm: $("#queryForm"), queryInput: $("#queryInput"), queryStatus: $("#queryStatus"), sendButton: $("#sendButton"),
  forceSearchInput: $("#forceSearchInput"), imageInput: $("#imageInput"), attachButton: $("#attachButton"),
  imagePreview: $("#imagePreview"), imageThumb: $("#imageThumb"), imageName: $("#imageName"), removeImageButton: $("#removeImageButton"),
  answerView: $("#answerView"), answerText: $("#answerText"), answerSource: $("#answerSource"),
  copyButton: $("#copyButton"), answerActions: $("#answerActions"), searchWebButton: $("#searchWebButton"), allowance: $("#allowance"),
  historySection: $("#historySection"), historyList: $("#historyList"), historyTemplate: $("#historyItemTemplate"),
  settingsButton: $("#settingsButton"), settingsPanel: $("#settingsPanel"), installButton: $("#installButton"),
  clearHistoryButton: $("#clearHistoryButton"), signOutButton: $("#signOutButton"), networkStatus: $("#networkStatus"),
  installHint: $("#installHint"), dismissInstallHint: $("#dismissInstallHint"),
};

let threads = loadThreads();
let activeThreadId = threads.at(-1)?.id || crypto.randomUUID();
let history = threads.at(-1)?.turns || [];
let attachedImage = null;
let attachedPreviewUrl = null;
let lastRequest = null;
let lastAnswer = null;
let deferredInstallPrompt = null;

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

function newConversation() {
  activeThreadId = crypto.randomUUID();
  history = [];
  lastRequest = null;
  restoreLatestAnswer();
  elements.queryInput.value = "";
  clearImage();
  elements.queryInput.focus();
}
$("#newConversationButton").addEventListener("click", newConversation);

function setActivated(value) {
  localStorage.setItem(ACTIVATED_KEY, value ? "true" : "false");
  elements.activationView.hidden = value;
  elements.mainView.hidden = !value;
  if (value) {
    renderHistory();
    restoreLatestAnswer();
  }
}

function restoreLatestAnswer() {
  const latest = history.at(-1);
  if (!latest) {
    lastAnswer = null;
    elements.answerView.classList.add("is-waiting");
    elements.answerSource.textContent = "READY";
    elements.answerText.textContent = "Your answer will appear here.";
    elements.copyButton.hidden = true;
    elements.answerActions.hidden = true;
    elements.searchWebButton.hidden = true;
    elements.allowance.textContent = "";
    return;
  }
  lastAnswer = latest.answer;
  elements.answerView.classList.remove("is-waiting");
  elements.answerSource.textContent = `${answerIndicators(latest)} ${latest.searched ? "FROM THE WEB" : "FROM KNOWLEDGE"}`.trim();
  elements.answerText.textContent = latest.answer;
  elements.copyButton.hidden = false;
  elements.answerActions.hidden = true;
  elements.searchWebButton.hidden = true;
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
  else button.querySelector("span").textContent = busy ? "WAIT…" : "ASK";
  if (!activation) {
    elements.queryInput.disabled = busy;
    elements.attachButton.disabled = busy;
    elements.searchWebButton.disabled = busy;
    $("#newConversationButton").disabled = busy;
    elements.clearHistoryButton.disabled = busy;
    elements.signOutButton.disabled = busy;
    elements.historyList.inert = busy;
  }
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    credentials: "same-origin",
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
  });
  let body = {};
  try { body = await response.json(); } catch { /* server returned no JSON */ }
  if (!response.ok) {
    if (response.status === 401 && path !== "/api/activate") setActivated(false);
    throw new Error(body.message || (response.status === 401 ? "Activation required" : "FindOut could not complete that request"));
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
  await ask({ query: elements.queryInput.value, forceSearch: elements.forceSearchInput.checked, image: attachedImage });
});

async function ask({ query, forceSearch, image, retry = false }) {
  elements.queryStatus.textContent = "";
  setBusy(true);
  try {
    if (!retry && history.length >= MAX_THREAD_TURNS) throw new Error("This thread has 100 answers. Start a new conversation to continue; this thread stays in Recent.");
    const payload = retry
      ? { ...lastRequest, force_search: true }
      : makeQueryPayload({ query, history, forceSearch, image, systemContext: mobileContext() });
    const { body, headers } = await api("/api/query", { method: "POST", body: JSON.stringify(payload) });
    if (typeof body.answer !== "string" || typeof body.searched !== "boolean") throw new Error("FindOut returned an invalid answer");
    if (retry && history.length) history = history.slice(0, -1);
    history = [...history, { query: payload.query, answer: body.answer, searched: body.searched, hadImage: Boolean(payload.image), timestamp: Date.now() }];
    saveHistory();
    lastRequest = payload;
    lastAnswer = body.answer;
    showAnswer(body.answer, body.searched, headers, Boolean(payload.image));
    elements.queryInput.value = "";
    elements.forceSearchInput.checked = false;
    resizeComposer();
    clearImage();
  } catch (error) {
    elements.queryStatus.textContent = error.message;
  } finally { setBusy(false); }
}

function showAnswer(answer, searched, headers = null, hadImage = false) {
  elements.answerView.classList.remove("is-waiting");
  elements.answerText.textContent = answer;
  elements.answerSource.textContent = `${answerIndicators({ searched, hadImage })} ${searched ? "FROM THE WEB" : "FROM KNOWLEDGE"}`.trim();
  elements.copyButton.hidden = false;
  elements.answerActions.hidden = false;
  elements.searchWebButton.hidden = searched;
  const remaining = headers?.get("x-findout-daily-remaining");
  const limit = headers?.get("x-findout-daily-limit");
  elements.allowance.textContent = remaining && limit ? `${remaining} OF ${limit} LEFT TODAY` : "";
  elements.answerView.scrollIntoView({ behavior: "smooth", block: "start" });
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
    item.querySelector(".history-title").textContent = `${turn.query} · ${thread.turns.length} ${thread.turns.length === 1 ? "answer" : "answers"}`;
    answer.textContent = threadText(thread.turns);
    const resume = document.createElement("button");
    resume.type = "button";
    resume.className = "text-button";
    resume.textContent = "CONTINUE THREAD";
    resume.addEventListener("click", () => {
      activeThreadId = thread.id;
      history = [...thread.turns];
      lastRequest = null;
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
}

elements.searchWebButton.addEventListener("click", () => {
  if (lastRequest) ask({ retry: true });
  else elements.queryStatus.textContent = "Ask a new question before retrying with web search";
});

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
  elements.queryStatus.textContent = "Preparing image…";
  try {
    attachedImage = await normalizeImage(file);
    if (attachedPreviewUrl) URL.revokeObjectURL(attachedPreviewUrl);
    attachedPreviewUrl = URL.createObjectURL(file);
    elements.imageThumb.src = attachedPreviewUrl;
    elements.imageName.textContent = file.name;
    elements.imagePreview.hidden = false;
    elements.queryStatus.textContent = "";
  } catch (error) {
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
document.addEventListener("click", (event) => {
  if (!elements.settingsPanel.hidden && !elements.settingsPanel.contains(event.target) && !elements.settingsButton.contains(event.target)) {
    elements.settingsPanel.hidden = true;
    elements.settingsButton.setAttribute("aria-expanded", "false");
  }
});

elements.clearHistoryButton.addEventListener("click", () => {
  history = [];
  threads = [];
  activeThreadId = crypto.randomUUID();
  saveHistory();
  lastRequest = null;
  restoreLatestAnswer();
  elements.settingsPanel.hidden = true;
  elements.settingsButton.setAttribute("aria-expanded", "false");
});

elements.signOutButton.addEventListener("click", async () => {
  try { await api("/api/logout", { method: "POST", body: "{}" }); } catch { /* local reset still applies */ }
  localStorage.removeItem(ACTIVATED_KEY);
  setActivated(false);
  elements.settingsPanel.hidden = true;
  elements.settingsButton.setAttribute("aria-expanded", "false");
});

function updateNetworkStatus() {
  const online = navigator.onLine;
  elements.networkStatus.classList.toggle("offline", !online);
  elements.networkStatus.lastElementChild.textContent = online ? "ONLINE" : "OFFLINE";
}
window.addEventListener("online", updateNetworkStatus);
window.addEventListener("offline", updateNetworkStatus);

window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  deferredInstallPrompt = event;
  elements.installButton.hidden = false;
});
elements.installButton.addEventListener("click", async () => {
  if (!deferredInstallPrompt) return;
  await deferredInstallPrompt.prompt();
  deferredInstallPrompt = null;
  elements.installButton.hidden = true;
});

const standalone = matchMedia("(display-mode: standalone)").matches || navigator.standalone === true;
const isiOS = /iphone|ipad|ipod/i.test(navigator.userAgent);
if (isiOS && !standalone && localStorage.getItem(INSTALL_HINT_KEY) !== "dismissed") elements.installHint.hidden = false;
elements.dismissInstallHint.addEventListener("click", () => {
  elements.installHint.hidden = true;
  localStorage.setItem(INSTALL_HINT_KEY, "dismissed");
});

if ("serviceWorker" in navigator) window.addEventListener("load", () => navigator.serviceWorker.register("/sw.js"));
setActivated(localStorage.getItem(ACTIVATED_KEY) === "true");
updateNetworkStatus();
renderHistory();

const feedbackDialog = $("#feedbackDialog");
$("#feedbackButton").addEventListener("click", () => feedbackDialog.showModal());
$("#feedbackClose").addEventListener("click", () => feedbackDialog.close());
$("#feedbackSave").addEventListener("click", () => {
  const blob = new Blob([$("#feedbackMessage").value, "\n\nReply email: ", $("#feedbackEmail").value], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a"); link.href = url; link.download = "findout-feedback.txt"; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
$("#feedbackForm").addEventListener("submit", async event => {
  event.preventDefault();
  if ($("#feedbackSend").disabled) return;
  $("#feedbackSend").disabled = true;
  for (const id of ["#feedbackMessage", "#feedbackEmail", "#feedbackLicense"]) $(id).disabled = true;
  $("#feedbackStatus").textContent = "Sending…";
  try {
    await api("/api/feedback", { method: "POST", signal: AbortSignal.timeout(15000), body: JSON.stringify({
      message: $("#feedbackMessage").value, email: $("#feedbackEmail").value.trim(),
      include_license: $("#feedbackLicense").checked, client: "web/0.1.5",
    }) });
    $("#feedbackMessage").value = "";
    $("#feedbackStatus").textContent = "Feedback sent. Thank you.";
  } catch (error) { $("#feedbackStatus").textContent = `${error.message}. Your text is kept here; you can also save it.`; }
  finally { $("#feedbackSend").disabled = false; for (const id of ["#feedbackMessage", "#feedbackEmail", "#feedbackLicense"]) $(id).disabled = false; }
});
