/** True when text is empty or only whitespace. */
export const isBlank = (text: string): boolean => text.trim() === "";

/** True when a saved or pasted decklist has content; whitespace is empty. */
export const hasDecklistText = (text: string): boolean => !isBlank(text);
