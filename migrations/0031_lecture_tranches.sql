-- Lecture par tranches des très gros PDF (suite du constat L2 de l'audit du 2026-11-26).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- Avancement d'une lecture en plusieurs parties (docs/lecture-tranches.md) : rang 0 = appel
-- d'en-tête et totaux, puis une ligne par tranche de pages du plan
-- (`gmflow_core::lecture::tranches::plan`). Une tâche de lecture relancée (panne, quota du jour,
-- redémarrage du worker) reprend les tranches déjà lues au lieu de tout relire. Les lignes sont
-- effacées quand la lecture se termine, et à chaque relecture demandée par un humain.
--   a_lire : pas encore lue ;
--   lue    : réponse gardée dans `reponse` (sortie structurée : l'extraction de l'en-tête au
--            rang 0, `{"lignes": [...]}` pour une tranche) ;
--   echec  : réponse inexploitable même au nouvel essai ; reprise à la prochaine tâche.
create table if not exists lecture_tranche (
    document_id bigint not null references document on delete cascade,
    rang        int not null,
    page_debut  int not null,
    page_fin    int not null,
    etat        text not null default 'a_lire' check (etat in ('a_lire', 'lue', 'echec')),
    reponse     jsonb null,
    modele      text null,
    essais      int not null default 0,
    message     text null,
    maj_le      timestamptz not null default now(),
    primary key (document_id, rang)
);
