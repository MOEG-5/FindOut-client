export const FOCUS_GRACE_MS = 10_000;

// Check elapsed time again on return: mobile browsers can suspend background timers.
export function watchFocusLoss(onReset, { window: appWindow = window, document: appDocument = document } = {}) {
  let awaySince = null;
  let reset = false;
  let timer = null;

  function resetIfExpired() {
    if (awaySince !== null && !reset && Date.now() - awaySince >= FOCUS_GRACE_MS) {
      reset = true;
      onReset();
    }
  }

  function leave() {
    if (awaySince !== null) return;
    awaySince = Date.now();
    reset = false;
    timer = setTimeout(resetIfExpired, FOCUS_GRACE_MS);
  }

  function returnToApp() {
    if (appDocument.hidden) return;
    resetIfExpired();
    clearTimeout(timer);
    timer = null;
    awaySince = null;
    reset = false;
  }

  function visibilityChanged() {
    if (appDocument.hidden) leave();
    else returnToApp();
  }

  appWindow.addEventListener("blur", leave);
  appWindow.addEventListener("focus", returnToApp);
  appWindow.addEventListener("pageshow", returnToApp);
  appDocument.addEventListener("visibilitychange", visibilityChanged);
  if (appDocument.hidden) leave();

  return () => {
    clearTimeout(timer);
    appWindow.removeEventListener("blur", leave);
    appWindow.removeEventListener("focus", returnToApp);
    appWindow.removeEventListener("pageshow", returnToApp);
    appDocument.removeEventListener("visibilitychange", visibilityChanged);
  };
}
