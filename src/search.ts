/** Lower case without accents, so that "reunion" finds "réunion". */
export function fold(text: string) {
  return text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}
