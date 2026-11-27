// Outils sans dépendance (testés par `npm test`) de la page Bilan du mois (audit U16).

/** Lien de téléchargement du bilan du mois en Excel (mêmes filtres que la page). */
export function lienExportBilan(mois: string, societe: number | null | undefined): string {
  const p = new URLSearchParams({ mois });
  if (societe !== null && societe !== undefined) p.set('societe', String(societe));
  return `/api/bilan/export?${p}`;
}
