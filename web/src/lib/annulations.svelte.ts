// Messages « Annuler » après une validation (U1, U2) : « Facture Vileda validée. Envoi à Dext
// dans 8 s. Annuler ». Le délai vient du serveur (`annulation.delai_s` de la réponse), qui tient
// l'envoi : fermer l'onglet ne l'empêche pas de partir. Pile courte : chaque validation garde
// son message et son « Annuler », les plus récents en bas ; ils disparaissent à la fin du délai.
// Affichés par `AnnulerEnvoi.svelte` (posé une fois dans la mise en page) ; Ctrl+Z annule le
// dernier.
import { annulerEnvoi, message as texteErreur } from './api.ts';
import { rafraichirCompteurs } from './compteurs.svelte.ts';
import type { Annulable } from './types.ts';

export type EtatMessage = 'attente' | 'annulation' | 'fini';

export interface MessageEnvoi {
  cle: number;
  lot: string;
  /** « Facture Vileda F-12 validée », « 14 factures validées ». */
  titre: string;
  /** « Dext », « Yooz ». */
  outil: string;
  /** Fin du délai (horloge du navigateur, d'après le délai du serveur). */
  fin: number;
  etat: EtatMessage;
  /** Après « Annuler » : ce qui s'est passé. */
  resultat: string;
  ton: 'info' | 'ok' | 'attention' | 'erreur';
}

/** Messages montrés à la fois au plus (les plus anciens restent annulables par Ctrl+Z…). */
export const MAX_VISIBLES = 3;
/** Durée d'affichage du résultat d'une annulation. */
const RESULTAT_MS = 6000;

let suivante = 1;

class Annulations {
  liste = $state<MessageEnvoi[]>([]);
  /** Rappelés après une annulation réussie (listes à relire). */
  #apres = new Set<() => void>();

  /** Une validation vient d'être faite : son message et son compte à rebours. */
  annoncer(a: Annulable, titre: string, outil: string) {
    const m: MessageEnvoi = {
      cle: suivante++,
      lot: a.lot,
      titre,
      outil,
      fin: Date.now() + a.delai_s * 1000,
      etat: 'attente',
      resultat: '',
      ton: 'info'
    };
    this.liste = [...this.liste, m];
    setTimeout(() => this.retirerSiAttente(m.cle), a.delai_s * 1000 + 250);
  }

  private retirerSiAttente(cle: number) {
    this.liste = this.liste.filter((m) => m.cle !== cle || m.etat !== 'attente');
  }

  fermer(cle: number) {
    this.liste = this.liste.filter((m) => m.cle !== cle);
  }

  /** S'abonne aux annulations réussies ; rend la fonction de désabonnement (pour `$effect`). */
  surAnnulation(f: () => void): () => void {
    this.#apres.add(f);
    return () => this.#apres.delete(f);
  }

  /** Dernier message encore annulable. */
  get dernier(): MessageEnvoi | undefined {
    return [...this.liste].reverse().find((m) => m.etat === 'attente');
  }

  async annuler(cle: number) {
    const m = this.liste.find((x) => x.cle === cle);
    if (!m || m.etat !== 'attente') return;
    this.maj(cle, { etat: 'annulation' });
    let resultat = '';
    let ton: MessageEnvoi['ton'] = 'ok';
    try {
      const r = await annulerEnvoi(m.lot);
      const n = r.annulees;
      if (n > 0 && r.deja_parties === 0) {
        resultat = r.pieces.length > 1 ? `Validation annulée : ${n} pièces de nouveau à contrôler.` : 'Validation annulée : de nouveau à contrôler.';
      } else if (n > 0) {
        resultat = `${n} validation${n > 1 ? 's' : ''} annulée${n > 1 ? 's' : ''}. Trop tard pour ${r.deja_parties} : déjà partie${r.deja_parties > 1 ? 's' : ''} à ${m.outil}.`;
        ton = 'attention';
      } else if (r.deja_parties > 0) {
        resultat = `Trop tard : déjà partie${r.deja_parties > 1 ? 's' : ''} à ${m.outil}.`;
        ton = 'attention';
      } else {
        resultat = 'Rien à annuler.';
        ton = 'attention';
      }
      if (n > 0) {
        void rafraichirCompteurs();
        for (const f of this.#apres) f();
      }
    } catch (e) {
      resultat = `Annulation impossible : ${texteErreur(e)}`;
      ton = 'erreur';
    }
    this.maj(cle, { etat: 'fini', resultat, ton });
    setTimeout(() => this.fermer(cle), RESULTAT_MS);
  }

  private maj(cle: number, champs: Partial<MessageEnvoi>) {
    this.liste = this.liste.map((m) => (m.cle === cle ? { ...m, ...champs } : m));
  }
}

export const annulations = new Annulations();
