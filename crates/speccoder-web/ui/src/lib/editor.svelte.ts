// The person's editor, one click away (spec 128): open a project file at a
// line in it through the host; with no editor yet, the Choose Editor sheet
// asks once and the open goes on.

import { app } from "./app.svelte";
import { post } from "./api";
import type { Opened, Reply } from "./types";

export interface OpenRequest {
  path: string;
  line?: number | null;
  column?: number | null;
  /** The feature whose branch's version to open, from its worktree. */
  feature?: string | null;
}

class EditorState {
  /** The sheet is open; `pending` opens once an editor is chosen. */
  choosing = $state<{ pending: OpenRequest | null } | null>(null);
}

export const editor = new EditorState();

/** Opens `r` in the person's editor and says where; asks for an editor
 *  first when none is chosen or found. */
export async function openIn(r: OpenRequest): Promise<void> {
  const body = { path: r.path, line: r.line ?? null, column: r.column ?? null, feature: r.feature === undefined ? app.feature : r.feature };
  let res: Reply<Opened>;
  try {
    res = await post<Reply<Opened>>("/api/editor/open", body);
  } catch (e) {
    app.toast({ tone: "error", text: String((e as Error).message ?? e) });
    return;
  }
  if (res.code === "EDITOR-NOT-FOUND") {
    editor.choosing = { pending: r };
    return;
  }
  if (res.status !== "ok" || !res.data) {
    app.toast({ tone: "warn", text: res.message });
    return;
  }
  const o = res.data;
  app.toast({ tone: "ok", text: `Opened ${o.path}${o.line ? `:${o.line}` : ""} in ${o.editor}`, detail: o.where === "worktree" ? "The feature branch's version, in its worktree." : undefined });
}
