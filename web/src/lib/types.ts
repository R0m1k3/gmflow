// Types de l'API : générés depuis les structures Rust (`web/src/lib/api-types/`, régénérés par
// `scripts/types-ts.sh`, vérifiés par la CI). Ne s'écrivent ici que les noms historiques du front
// et les types propres à l'interface.
export type * from './api-types/index.ts';

import type {
  AppelVue,
  BilanLecture,
  BrouillonVue,
  DemandeEssai,
  Echec,
  Eligibilite,
  EnvoiVue,
  LigneEdit,
  LigneSaisie,
  LigneVue,
  Marque,
  Validation
} from './api-types/index.ts';

/** Réponse proposée d'un mail (`FicheMail.brouillons`). */
export type Brouillon = BrouillonVue;
/** Expéditeur marqué « publicité » (Réglages > Tri). */
export type ExpediteurMarque = Marque;
/** Alerte d'un document (`controles`). */
export type Controle = Echec;
/** Envoi à Dext/Yooz (fiche d'un document). */
export type Envoi = EnvoiVue;
/** Justesse de la lecture (Bilan). */
export type Bilan = BilanLecture;
/** Appel à Claude (Réglages > Claude). */
export type AppelClaude = AppelVue;
/** Ligne article d'un document (rapprochement). */
export type LigneArticle = LigneVue;
/** Valeurs d'une ligne article qu'on corrige à la main (`/api/documents/{id}/lignes`). */
export type SaisieLigne = LigneSaisie;
export type LigneEdition = LigneEdit;
/** Réponse de « Valider » (`POST /api/documents/{id}/valider`). */
export type ResultatValidation = Validation;
/** `POST /api/documents/lot/eligibilite` : la pièce peut-elle être validée avec les autres. */
export type EligibiliteLot = Eligibilite;
/** Corps de `POST /api/admin/boites/tester`. */
export type DemandeEssaiBoite = DemandeEssai;

// ---------------------------------------------------------------------------------------------
// Codes d'erreur de l'API (`code` du corps d'erreur) : voir erreurs.ts.

export type CodeRefus =
  | 'donnees'
  | 'champ'
  | 'non_connecte'
  | 'interdit'
  | 'absent'
  | 'conflit'
  | 'trop_d_essais'
  | 'interne'
  | 'serveur_magasin_absent'
  | 'logiflow_absent'
  | 'etat'
  | 'deja_envoye'
  | 'expedition'
  | 'erreur_envoi'
  | 'doublon'
  | 'sans_adresse'
  | 'smtp_absent'
  | 'essai'
  | 'alertes';
