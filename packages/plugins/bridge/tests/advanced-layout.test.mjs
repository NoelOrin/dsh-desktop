import assert from "node:assert/strict";
import test from "node:test";
import { computeDesktopColumns } from "../lib/advanced.js";

test("advanced layout collapses details on narrow viewport", () => {
  const columns = computeDesktopColumns(
    { sidebarOpen: true, detailsOpen: true, sidebarWidth: 280, detailsWidth: 360 },
    { width: 800 },
  );
  assert.equal(columns.sidebar, 280);
  assert.equal(columns.details, 0);
  assert.ok(columns.conversation > 0);
});

test("advanced layout keeps sidebar and details on desktop viewport", () => {
  const columns = computeDesktopColumns(
    { sidebarOpen: true, detailsOpen: true, sidebarWidth: 280, detailsWidth: 360 },
    { width: 1440 },
  );
  assert.equal(columns.sidebar, 280);
  assert.equal(columns.details, 360);
  assert.ok(columns.conversation > 360);
});

test("advanced layout collapses sidebar on demand", () => {
  const columns = computeDesktopColumns(
    { sidebarOpen: false, detailsOpen: true, sidebarWidth: 280, detailsWidth: 360 },
    { width: 1440 },
  );
  assert.equal(columns.sidebar, 0);
  assert.equal(columns.details, 360);
});
