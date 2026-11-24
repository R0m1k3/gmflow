<script lang="ts" module>
  // Jeu d'icônes au trait (dessins de type lucide), 18 px, trait 1,5 : celles de la maquette Sauge.
  const TRACES = {
    maison: '<path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>',
    serveur: '<path d="M4 4h16v6H4zM4 14h16v6H4zM8 7h.01M8 17h.01"/>',
    utilisateur: '<path d="M12 12a4 4 0 1 0 0-8a4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0"/>',
    chercher: '<path d="M4 11a7 7 0 1 0 14 0a7 7 0 1 0-14 0M20 20l-4-4"/>',
    reglages:
      '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
    bas: '<path d="M6 9l6 6 6-6"/>',
    droite: '<path d="M9 6l6 6-6 6"/>',
    gauche: '<path d="m15 6-6 6 6 6"/>',
    haut: '<path d="m18 15-6-6-6 6"/>',
    alerte: '<path d="M12 3.5 2.5 20h19zM12 10v4.5M12 17.2v.1"/>',
    erreur: '<circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/>',
    info: '<path d="M12 3a9 9 0 1 0 .01 0zM12 11v5.5M12 7.8v.1"/>',
    coche: '<path d="M4 12.5l5 5L20 6.5"/>',
    trombone: '<path d="M20.5 11.5 12.4 19.6a5 5 0 0 1-7-7L13.9 4a3.3 3.3 0 0 1 4.7 4.7l-8.4 8.4a1.7 1.7 0 0 1-2.4-2.4l7.6-7.6"/>',
    fichier: '<path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8zM14 3v5h5M9 13h6M9 17h6"/>',
    avoir: '<path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8zM14 3v5h5M9 15h6"/>',
    agrandir: '<path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>',
    zoom: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5M11 8v6M8 11h6"/>',
    telecharger: '<path d="M12 3v12m0 0-4-4m4 4 4-4M4 21h16"/>',
    mail: '<path d="M3 6h18v12H3zM3 7l9 6 9-6"/>',
    boite:
      '<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>',
    camion: '<path d="M2 6h12v10H2zM14 10h4l3 3v3h-7M5 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0M15 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0"/>',
    graphique: '<path d="M4 20V11M10 20V5M16 20v-6M3 20h18"/>',
    batiment: '<path d="M4 21V5a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v16M15 9h4a1 1 0 0 1 1 1v11M3 21h18M8 8h3M8 12h3M8 16h3"/>',
    prise: '<path d="M12 22v-5M9 8V2M15 8V2M18 8v4a6 6 0 0 1-12 0V8z"/>',
    envoyer: '<path d="M22 2 11 13M22 2l-7 20-4-9-9-4z"/>',
    utilisateurs:
      '<circle cx="9" cy="8" r="4"/><path d="M2 21a7 7 0 0 1 14 0M16 4a4 4 0 0 1 0 8M22 21a7 7 0 0 0-4-6.3"/>',
    curseurs: '<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1M13 6a2 2 0 1 0 4 0a2 2 0 1 0-4 0M7 12a2 2 0 1 0 4 0a2 2 0 1 0-4 0M15 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0"/>',
    drapeau: '<path d="M5 21V4M5 4h11l-2 4 2 4H5"/>',
    lien: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
    menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
    fermer: '<path d="M6 6l12 12M18 6 6 18"/>',
    sortir: '<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"/>',
    ouvrir: '<path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
    crayon: '<path d="M17 3a2.8 2.8 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5z"/><path d="m15 5 4 4"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    rafraichir: '<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7"/>'
  } as const;
  export type NomIcone = keyof typeof TRACES;
</script>

<script lang="ts">
  let { nom, titre, class: classe = '' }: { nom: NomIcone; titre?: string; class?: string } = $props();
</script>

<svg class="i {classe}" viewBox="0 0 24 24" aria-hidden={titre ? undefined : 'true'} role={titre ? 'img' : undefined}>
  {#if titre}<title>{titre}</title>{/if}
  {@html TRACES[nom]}
</svg>
