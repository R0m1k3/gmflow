//! Contrôles avant dépôt : identifiants, montants, société, fournisseur, dates.

use gmflow_core::controles::{
    Facture, cle_tva_fr, controler, siren_valide, siret_valide, tva_valide,
};
use gmflow_core::montant::dec;

fn codes(f: &Facture<'_>, jour: time::Date) -> Vec<String> {
    controler(f, jour).into_iter().map(|e| e.code).collect()
}

fn jour() -> time::Date {
    time::macros::date!(2026 - 11 - 20)
}

fn bonne() -> Facture<'static> {
    Facture {
        societe_id: Some(1),
        confiance_societe: 1.0,
        fournisseur_statut: Some("valide"),
        fournisseur_siret: Some("55210055400013"),
        fournisseur_tva: Some("FR96552100554"),
        numero: Some("F-1"),
        date: Some(time::macros::date!(2026 - 11 - 12)),
        ht: Some(dec!(100.0)),
        tva: Some(dec!(20.0)),
        ttc: Some(dec!(120.0)),
        adresse_depot: true,
        ..Default::default()
    }
}

#[test]
fn identifiants() {
    assert!(siret_valide("552 100 554 00013"));
    assert!(siret_valide("73282932000074"));
    assert!(!siret_valide("73282932000075"));
    assert!(!siret_valide("7328293200007"));
    // La Poste : somme des chiffres multiple de 5.
    assert!(siret_valide("35600000000001"));
    assert!(!siret_valide("35600000000002"));
    assert!(siren_valide("552100554"));
    assert!(!siren_valide("552100555"));
    assert_eq!(cle_tva_fr("552100554"), Some(96));
    assert_eq!(cle_tva_fr("380129866"), Some(89));
    assert!(tva_valide("FR 96 552100554"));
    assert!(!tva_valide("FR40552100554"));
    assert!(tva_valide("DE123456789"));
    assert!(!tva_valide("123456789"));
}

#[test]
fn facture_conforme() {
    assert!(controler(&bonne(), jour()).is_empty());
}

#[test]
fn chaque_controle() {
    let mut f = bonne();
    f.ttc = Some(dec!(120.06));
    assert_eq!(codes(&f, jour()), ["montants"]);
    f.ttc = Some(dec!(120.05));
    assert!(
        codes(&f, jour()).is_empty(),
        "0,05 € toléré (seuil partagé)"
    );

    let mut f = bonne();
    f.fournisseur_siret = Some("55210055400014");
    assert_eq!(codes(&f, jour()), ["identifiants"]);
    // SIREN seul (9 chiffres), demandé à la lecture à défaut de SIRET : accepté s'il est juste.
    f.fournisseur_siret = Some("552 100 554");
    assert!(codes(&f, jour()).is_empty(), "SIREN valide accepté");
    f.fournisseur_siret = Some("552100555");
    assert_eq!(codes(&f, jour()), ["identifiants"]);

    let mut f = bonne();
    f.confiance_societe = 0.85;
    assert_eq!(codes(&f, jour()), ["societe"]);
    f.societe_id = None;
    assert_eq!(codes(&f, jour()), ["societe"]);

    let mut f = bonne();
    f.adresse_depot = false;
    assert_eq!(codes(&f, jour()), ["depot"]);

    let mut f = bonne();
    f.fournisseur_statut = Some("a_confirmer");
    assert_eq!(codes(&f, jour()), ["fournisseur"]);
    f.fournisseur_statut = None;
    assert_eq!(codes(&f, jour()), ["fournisseur"]);

    let mut f = bonne();
    f.date = Some(time::macros::date!(2026 - 11 - 21));
    assert_eq!(codes(&f, jour()), ["date"]);
    f.date = Some(time::macros::date!(2025 - 10 - 01));
    assert_eq!(codes(&f, jour()), ["date"]);

    let mut f = bonne();
    f.numero = None;
    f.date = None;
    assert_eq!(codes(&f, jour()), ["incomplet"]);
}

#[test]
fn messages_dans_le_vocabulaire_de_l_ecran() {
    use gmflow_core::controles::montant_fr;
    assert_eq!(montant_fr(dec!(1250.4)), "1\u{a0}250,40\u{a0}€");
    assert_eq!(montant_fr(dec!(60.0)), "60,00\u{a0}€");
    assert_eq!(
        montant_fr(dec!(1234567.891)),
        "1\u{a0}234\u{a0}567,89\u{a0}€"
    );
    assert_eq!(montant_fr(-dec!(12.5)), "-12,50\u{a0}€");

    let mut f = bonne();
    f.ht = Some(dec!(1000.0));
    f.tva = Some(dec!(200.0));
    f.ttc = Some(dec!(1250.4));
    let e = controler(&f, jour());
    assert_eq!(
        e[0].message,
        "HT + TVA donnent 1\u{a0}200,00\u{a0}€, le TTC lu est 1\u{a0}250,40\u{a0}€ \
         (éco-participation ou port à ajouter au HT et à la TVA ?)"
    );
    assert_eq!(e[0].champs, ["montant_ttc"]);
    // TTC plus grand (éco-participation oubliée) : pas de baisse du TTC proposée.
    assert!(e[0].proposition.is_none());
    f.ttc = Some(dec!(1150.4));
    let e = controler(&f, jour());
    assert_eq!(
        e[0].message,
        "HT + TVA donnent 1\u{a0}200,00\u{a0}€, le TTC lu est 1\u{a0}150,40\u{a0}€"
    );
    let p = e[0].proposition.as_ref().unwrap();
    assert_eq!((p.champ.as_str(), p.valeur), ("montant_ttc", dec!(1200.0)));

    f.ht = None;
    f.ttc = Some(dec!(1250.4));
    let e = controler(&f, jour());
    let p = e[0].proposition.as_ref().unwrap();
    assert_eq!((p.champ.as_str(), p.valeur), ("montant_ht", dec!(1050.4)));

    let mut f = bonne();
    f.societe_id = None;
    f.fournisseur_statut = Some("a_confirmer");
    f.date = Some(time::macros::date!(2026 - 11 - 21));
    let m: Vec<String> = controler(&f, jour())
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(
        m,
        [
            "Société destinataire non reconnue",
            "Nouveau fournisseur (à confirmer)",
            "Date du 21/11/2026 dans le futur"
        ]
    );
    let mut f = bonne();
    f.adresse_depot = false;
    let e = controler(&f, jour());
    assert!(!e[0].message.contains("structure") && e[0].message.contains("Dext"));
}

#[test]
fn controles_de_coherence() {
    // Échéance avant la date du document.
    let mut f = bonne();
    f.echeance = Some(time::macros::date!(2026 - 11 - 01));
    assert_eq!(codes(&f, jour()), ["echeance"]);
    assert_eq!(
        controler(&f, jour())[0].message,
        "Échéance du 01/11/2026 avant la date du document (12/11/2026)"
    );
    f.echeance = Some(time::macros::date!(2026 - 12 - 12));
    assert!(codes(&f, jour()).is_empty());

    // SIRET et TVA lus ≠ fiche fournisseur ; un autre établissement (même SIREN) passe.
    let mut f = bonne();
    f.siret_lu = Some("73282932000074");
    f.sirets_fiche = vec!["55210055400013"];
    assert_eq!(codes(&f, jour()), ["identite"]);
    f.siret_lu = Some("552 100 554");
    assert!(codes(&f, jour()).is_empty(), "même SIREN");
    f.sirets_fiche = vec![];
    f.siret_lu = Some("73282932000074");
    assert!(
        codes(&f, jour()).is_empty(),
        "fiche sans SIRET : rien à comparer"
    );
    let mut f = bonne();
    f.tva_lue = Some("FR 89 380129866");
    f.tvas_fiche = vec!["FR96552100554"];
    assert_eq!(codes(&f, jour()), ["identite"]);
    f.tvas_fiche = vec!["FR96552100554", "FR89380129866"];
    assert!(codes(&f, jour()).is_empty(), "TVA du groupe");

    // Devise.
    let mut f = bonne();
    f.devise = Some("USD");
    assert_eq!(codes(&f, jour()), ["devise"]);
    f.devise = Some("EUR");
    assert!(codes(&f, jour()).is_empty());

    // TVA au-dessus de 20 % du HT.
    let mut f = bonne();
    f.tva = Some(dec!(25.0));
    f.ttc = Some(dec!(125.0));
    assert_eq!(codes(&f, jour()), ["tva"]);
}

#[test]
fn somme_des_lignes_et_ht() {
    let mut f = bonne();
    f.somme_lignes = Some(dec!(100.0));
    assert!(codes(&f, jour()).is_empty());
    f.somme_lignes = Some(dec!(90.0));
    assert_eq!(codes(&f, jour()), ["lignes"]);
    // Remise de pied de 10 % déduite après les lignes : expliqué.
    let mut f = bonne();
    f.somme_lignes = Some(dec!(111.11));
    f.remise_ht = Some(dec!(11.11));
    assert!(codes(&f, jour()).is_empty());
    f.remise_ht = None;
    f.remise_taux = Some(dec!(10.0));
    f.somme_lignes = Some(dec!(111.1));
    assert!(codes(&f, jour()).is_empty(), "remise en taux");
    // Éco-participation ajoutée au HT (annexes lues à part, ou par unité sur les lignes).
    let mut f = bonne();
    f.somme_lignes = Some(dec!(97.5));
    f.annexes_ht = Some(dec!(2.5));
    assert!(codes(&f, jour()).is_empty());
    f.annexes_ht = None;
    f.eco_lignes = dec!(2.5);
    assert!(codes(&f, jour()).is_empty());
    // Remise et port ensemble.
    let mut f = bonne();
    f.somme_lignes = Some(dec!(105.0));
    f.remise_ht = Some(dec!(10.0));
    f.annexes_ht = Some(dec!(5.0));
    assert!(codes(&f, jour()).is_empty());
    f.somme_lignes = Some(dec!(150.0));
    assert_eq!(codes(&f, jour()), ["lignes"]);
}
