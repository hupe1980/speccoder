// The comment tray of the current feature (spec 093): the host keeps the
// drafts in its state directory, so they survive a reload and the terminal
// lists them too. Every change is an action through the job endpoint; the
// tray reads the host's list again when a job ends.

import { app } from "./app.svelte";
import { offer } from "./actions";
import { get, q } from "./api";
import type { CommentAnchor, CommentsView, Reply } from "./types";
import { place } from "./review";

class Comments {
  view = $state<CommentsView | null>(null);
  /** The tray is open (a chip otherwise). */
  open = $state(false);
  /** Why the host could not read the tray: said, never shown as empty. */
  error = $state<string | null>(null);

  async load() {
    try {
      const r = await get<Reply<CommentsView>>(`/api/review/comments${q({ feature: app.feature ?? undefined })}`);
      this.view = r.data ?? null;
      this.error = r.data || r.status === "ok" || r.status === "selection_needed" ? null : (r.message ?? null);
    } catch (e) {
      this.view = null;
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  get count(): number {
    return this.view?.drafts.length ?? 0;
  }

  add(anchor: CommentAnchor, text: string) {
    return app.perform(
      offer(
        { kind: "comment", op: "add", anchor, id: null },
        { group: "review", label: `comment on ${place(anchor)}`, why: "kept in the tray until you send it", command: `speccoder review comments add … "…"` },
      ),
      text,
    );
  }

  drop(id: string) {
    return app.perform(offer({ kind: "comment", op: "delete", anchor: null, id }, { group: "review", label: `delete ${id}`, why: "it leaves the tray", command: `speccoder review comments delete ${id}` }));
  }

  keep(id: string) {
    return app.perform(offer({ kind: "comment", op: "keep", anchor: null, id }, { group: "review", label: `keep ${id}`, why: "kept as it is", command: `speccoder review comments keep ${id}` }));
  }

  send() {
    const ids = (this.view?.drafts ?? []).filter((d) => d.state === "draft").map((d) => d.id);
    return app.perform(
      offer({ kind: "comments_send", ids }, { group: "review", label: `send ${ids.length} comment(s)`, why: "one revision, limited to the sections they touch", command: "speccoder review comments send" }),
    );
  }

  undo() {
    const turn = this.view?.turn?.id ?? null;
    return app.perform(offer({ kind: "revision_undo", turn }, { group: "review", label: "undo the revision", why: "the documents as they were, byte for byte", command: "speccoder review comments undo" }));
  }
}

export const comments = new Comments();
