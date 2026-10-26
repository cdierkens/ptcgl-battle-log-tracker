/** True when a saved or pasted decklist has content; whitespace is empty. */
export const hasDecklistText = (text: string): boolean => text.trim() !== "";
