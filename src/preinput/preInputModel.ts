export const MAX_OVERLAY_CHARACTERS = 180;

export type PreInputTextSegments = {
  confirmedText: string;
  hiddenPrefix: boolean;
  pendingText: string;
};

export function getPreInputTextSegments(
  text: string,
  confirmedChars = 0,
  maxCharacters = MAX_OVERLAY_CHARACTERS,
): PreInputTextSegments {
  const characters = Array.from(text);
  const hiddenPrefixChars = Math.max(0, characters.length - maxCharacters);
  const visibleCharacters = characters.slice(hiddenPrefixChars);
  const safeConfirmedChars = Math.min(Math.max(confirmedChars, 0), characters.length);
  const visibleConfirmedChars = Math.max(0, safeConfirmedChars - hiddenPrefixChars);

  return {
    hiddenPrefix: hiddenPrefixChars > 0,
    confirmedText: visibleCharacters.slice(0, visibleConfirmedChars).join(""),
    pendingText: visibleCharacters.slice(visibleConfirmedChars).join(""),
  };
}
