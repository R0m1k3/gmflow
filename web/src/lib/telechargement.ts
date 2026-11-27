// Nom d'un fichier téléchargé, lu dans l'en-tête `Content-Disposition` du serveur
// (`fichiers::disposition` : `filename="ascii"; filename*=UTF-8''encodé`). Module pur : testé par
// `node --test` (telechargement.test.ts).

export function nomFichier(disposition: string | null, defaut: string): string {
  if (!disposition) return defaut;
  const utf8 = /filename\*\s*=\s*UTF-8''([^;]+)/i.exec(disposition);
  if (utf8) {
    try {
      return decodeURIComponent(utf8[1].trim());
    } catch {
      /* encodage invalide : nom ASCII */
    }
  }
  const ascii = /filename\s*=\s*"([^"]*)"/i.exec(disposition) ?? /filename\s*=\s*([^;]+)/i.exec(disposition);
  return ascii?.[1].trim() || defaut;
}
