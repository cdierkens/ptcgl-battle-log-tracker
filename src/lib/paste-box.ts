import type { SubmitOutcome } from "./tauri";

import { isBlank } from "./decklist-text";

export type PasteAction =
  | { message: string; type: "failed" }
  | { text: string; type: "edit" }
  | { type: "saved" }
  | { type: "submit" };

export interface PasteModel {
  error: null | string;
  submitting: boolean;
  text: string;
}

export const ALREADY_SAVED_MESSAGE = "That Battle Log was already saved. Export this match's Battle Log and paste it.";
export const DUPLICATE_LOG_MESSAGE = "That Battle Log is already saved.";

export const initialPaste: PasteModel = { error: null, submitting: false, text: "" };

/**
 * For a match (its overlay state), a re-paste only counts once that match's log is saved.
 * For any log (null), a re-paste is a duplicate.
 */
export function actionForOutcome(outcome: SubmitOutcome, matchState: null | string): PasteAction {
  if (outcome.kind !== "alreadySaved") return { type: "saved" };
  if (matchState === null) return { message: DUPLICATE_LOG_MESSAGE, type: "failed" };
  if (matchState === "Saved") return { type: "saved" };
  return { message: ALREADY_SAVED_MESSAGE, type: "failed" };
}

export { isBlank };
export function pasteReducer(model: PasteModel, action: PasteAction): PasteModel {
  switch (action.type) {
    case "edit":
      return { ...model, error: null, text: action.text };
    case "failed":
      return { ...model, error: action.message, submitting: false };
    case "saved":
      return initialPaste;
    case "submit":
      return isBlank(model.text)
        ? { ...model, error: "Paste your Battle Log first." }
        : { ...model, error: null, submitting: true };
  }
}
