// SPDX-License-Identifier: AGPL-3.0-or-later
// Which window each feature in the story shows, and the one line under it.
import type { Start } from "./KitabApp";

export type Scene = { start: Start; label: string; hint: string };

/** The hero's product shot: a book open, two more documents in tabs. Not part of the story. */
export const HERO: Omit<Scene, "hint"> = {
  start: { tabs: ["pride", "welcome", "notes"], active: "pride", sidebar: false },
  label: "Kitab reading Pride and Prejudice, with two more documents open in tabs and the dock floating under the page",
};

export const SCENES: Record<string, Scene> = {
  pages: {
    start: { tabs: ["sizes", "welcome"], active: "sizes", sidebar: false, zoom: 25 },
    label: "Kitab showing a PDF whose pages are A4 portrait, A4 landscape, A5, Letter and a receipt, each at its own size",
    hint: "Zoom with − and +, or press fit width. The pages keep their shapes.",
  },
  speed: {
    start: { tabs: ["welcome", "pride", "notes"], active: "welcome", sidebar: true },
    label: "Kitab reading the Welcome to Kitab guide, with its contents in the sidebar and the dock at the bottom",
    hint: "Drag the page slider in the dock, or click the page number and type one.",
  },
  tabs: {
    start: { tabs: ["welcome", "pride", "notes"], active: null },
    label: "Kitab's Home with Continue reading and the recent books, and three documents open in tabs",
    hint: "Point at a book, open it, and move between the tabs. The house button goes back to Home.",
  },
  find: {
    start: { tabs: ["welcome", "notes"], active: "welcome", sidebar: true, find: "page" },
    label: "Find in Kitab: every hit for the word page highlighted, the find bar counting them",
    hint: "Type in the find bar. Enter steps to the next hit, Shift+Enter goes back.",
  },
  markdown: {
    start: { tabs: ["notes", "welcome"], active: "notes", sidebar: true },
    label: "A Markdown file in Kitab with a flowchart drawn in the theme's colours, a code block, a table and a task list",
    hint: "Copy the code, or press Edit to see the source beside the preview.",
  },
  forms: {
    start: { tabs: ["welcome"], active: "welcome", sidebar: false, markup: true, page: 2 },
    label: "Kitab's markup bar over a PDF page, with the highlighter picked and a highlight on the page",
    hint: "Drag across the page to draw. Save, in the dock, writes the marks into the PDF.",
  },
  privacy: {
    start: { tabs: ["welcome"], active: "welcome", sidebar: true, settings: "About" },
    label: "Kitab's Settings, About: the update check and what it sends",
    hint: "Open Appearance and pick a theme. Every window on this page follows it.",
  },
};
