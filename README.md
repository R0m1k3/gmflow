# GMFlow

Collecte et tri des factures et bons de livraison reçus par mail par les structures
(Frouard, Houdemont) : relève IMAP, consultation des mails et pièces jointes, puis (lots suivants)
lecture des documents, dépôt dans Dext ou Yooz et rapprochement des BL.

- Lot 1 (socle) : relève des boîtes toutes les 5 minutes, reprise de l'historique, liste et fiche des
  mails, comptes et droits par structure, administration, jeton Claude.
- Stack : Rust (axum, sqlx, apalis), PostgreSQL 16, SvelteKit, Docker (GHCR + Portainer).

Installation et conventions : voir [CLAUDE.md](CLAUDE.md) et [.env.example](.env.example).
