import assert from "node:assert/strict";
import test from "node:test";
import { FOCUS_GRACE_MS, watchFocusLoss } from "../focus.js";

function setup(t) {
  t.mock.timers.enable({ apis: ["Date", "setTimeout"], now: 1000 });
  const window = new EventTarget();
  const document = new EventTarget();
  document.hidden = false;
  let resets = 0;
  const stop = watchFocusLoss(() => resets++, { window, document });
  t.after(stop);
  return {
    window, document, stop,
    get resets() { return resets; },
    hide() { document.hidden = true; document.dispatchEvent(new Event("visibilitychange")); },
    show() { document.hidden = false; document.dispatchEvent(new Event("visibilitychange")); },
  };
}

test("a return within the desktop grace period preserves the active thread", t => {
  const app = setup(t);
  app.window.dispatchEvent(new Event("blur"));
  t.mock.timers.tick(FOCUS_GRACE_MS - 1);
  app.window.dispatchEvent(new Event("focus"));
  t.mock.timers.tick(FOCUS_GRACE_MS);
  assert.equal(app.resets, 0);
});

test("blur and mobile visibility loss share one deadline and reset once", t => {
  const app = setup(t);
  app.window.dispatchEvent(new Event("blur"));
  t.mock.timers.tick(6000);
  app.hide();
  t.mock.timers.tick(4000);
  assert.equal(app.resets, 1);
  app.show();
  app.window.dispatchEvent(new Event("focus"));
  assert.equal(app.resets, 1);
  app.hide();
  t.mock.timers.tick(FOCUS_GRACE_MS);
  assert.equal(app.resets, 2);
});

test("returning resets an expired thread even if background timers were suspended", t => {
  const app = setup(t);
  app.hide();
  t.mock.timers.setTime(1000 + FOCUS_GRACE_MS);
  app.window.dispatchEvent(new Event("focus"));
  app.show();
  assert.equal(app.resets, 1);
  t.mock.timers.tick(FOCUS_GRACE_MS);
  assert.equal(app.resets, 1);
});

test("moving between controls does not count as leaving the app", t => {
  const app = setup(t);
  app.document.dispatchEvent(new Event("focusout"));
  t.mock.timers.tick(FOCUS_GRACE_MS);
  assert.equal(app.resets, 0);
});

test("stopping the watcher removes listeners and cancels its deadline", t => {
  const app = setup(t);
  app.hide();
  app.stop();
  t.mock.timers.tick(FOCUS_GRACE_MS);
  app.window.dispatchEvent(new Event("blur"));
  t.mock.timers.tick(FOCUS_GRACE_MS);
  assert.equal(app.resets, 0);
});
