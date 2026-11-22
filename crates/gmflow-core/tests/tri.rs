//! Tri des mails et construction des réponses : fonctions pures.

use gmflow_core::reponse::{self, Original};
use gmflow_core::tri::{self, ContexteRegle, Entetes, Marque};
use serde_json::json;

fn regle(
    expediteur: &str,
    entetes: &Entetes,
    marques: &[Marque],
    corps: &str,
    pieces: usize,
) -> Option<&'static str> {
    let internes = vec!["factures@frouard.test".to_string()];
    tri::par_regle(&ContexteRegle {
        expediteur,
        entetes,
        marques,
        adresses_internes: &internes,
        corps,
        pieces_utiles: pieces,
    })
    .map(|(c, _)| c)
}

const QUESTION: &str = "Bonjour,\n\nPourriez-vous me dire quand la commande 4471 sera livrée ? \
Le client attend depuis deux semaines.\n\nCordialement,\nJean";

#[test]
fn regles_sans_ia() {
    let rien = Entetes::default();
    // Une vraie question d'une personne : aucune règle, Claude tranche.
    assert_eq!(regle("jean@exemple.test", &rien, &[], QUESTION, 0), None);

    let liste = Entetes {
        liste_diffusion: true,
        ..Default::default()
    };
    assert_eq!(
        regle("promo@exemple.test", &liste, &[], QUESTION, 0),
        Some("publicite")
    );

    let marques = [Marque::Domaine("pub.test".into())];
    assert_eq!(
        regle("offres@news.pub.test", &rien, &marques, QUESTION, 0),
        Some("publicite")
    );
    let marques = [Marque::Adresse("Offres@Exemple.test".into())];
    assert_eq!(
        regle("offres@exemple.test", &rien, &marques, QUESTION, 0),
        Some("publicite")
    );

    for exp in [
        "no-reply@transporteur.test",
        "noreply@x.test",
        "ne-pas-repondre@banque.test",
        "MAILER-DAEMON@x.test",
        "notifications@outil.test",
    ] {
        assert_eq!(
            regle(exp, &rien, &[], QUESTION, 0),
            Some("notification"),
            "{exp}"
        );
    }
    let auto = Entetes {
        auto_submitted: Some("auto-replied".into()),
        ..Default::default()
    };
    assert_eq!(
        regle("jean@exemple.test", &auto, &[], QUESTION, 0),
        Some("notification")
    );
    let auto_no = Entetes {
        auto_submitted: Some("no".into()),
        ..Default::default()
    };
    assert_eq!(regle("jean@exemple.test", &auto_no, &[], QUESTION, 0), None);
    let bulk = Entetes {
        precedence: Some("bulk".into()),
        ..Default::default()
    };
    assert_eq!(
        regle("jean@exemple.test", &bulk, &[], QUESTION, 0),
        Some("notification")
    );

    // Envoyé depuis une de nos boîtes.
    assert_eq!(
        regle("Factures@Frouard.test", &rien, &[], QUESTION, 0),
        Some("information")
    );
}

#[test]
fn document_seul_sans_jamais_avaler_une_question() {
    let rien = Entetes::default();
    let court = "Bonjour,\n\nCi-joint notre facture.\n\nCordialement,\nSophie Martin\nGrossiste\nTél 03 00 00 00 00";
    assert_eq!(
        regle("compta@grossiste.test", &rien, &[], court, 1),
        Some("document_seul")
    );
    // Même texte sans pièce jointe : pas de règle.
    assert_eq!(regle("compta@grossiste.test", &rien, &[], court, 0), None);
    // Un texte court avec un point d'interrogation reste pour Claude (question possible).
    assert_eq!(
        regle(
            "compta@grossiste.test",
            &rien,
            &[],
            "Pouvez-vous valider ?",
            1
        ),
        None
    );
    // Une facture envoyée par un no-reply avec un texte court : document seul.
    assert_eq!(
        regle("no-reply@grossiste.test", &rien, &[], "Votre facture", 1),
        Some("document_seul")
    );
}

#[test]
fn corps_utile_retire_citation_et_signature() {
    let corps = "Bonjour,\n\nMerci de rappeler le client.\n\nCordialement\nJean\n\n\
                 Le 12 nov. 2026 à 10:00, Paul <paul@x.test> a écrit :\n> ancienne question\n> suite";
    assert_eq!(tri::corps_utile(corps), "Merci de rappeler le client.");
    let outlook = "Je confirme.\n\nDe : Paul\nEnvoyé : mardi\nÀ : Jean\nObjet : commande";
    assert_eq!(tri::corps_utile(outlook), "Je confirme.");
    let sig = "Vu.\n-- \nJean Dupont";
    assert_eq!(tri::corps_utile(sig), "Vu.");
    assert_eq!(
        tri::sans_citation("Bonjour,\nOK.\nCordialement\nJean\n> cité"),
        "Bonjour,\nOK.\nCordialement\nJean"
    );
}

#[test]
fn entetes_du_mail() {
    let brut = b"From: Jean <jean@exemple.test>\r\nReply-To: support@exemple.test\r\n\
Message-ID: <abc@exemple.test>\r\nReferences: <r1@x.test> <r2@x.test>\r\n\
Auto-Submitted: auto-generated\r\nPrecedence: bulk\r\nList-Unsubscribe: <mailto:u@x.test>\r\n\
Subject: test\r\n\r\ncorps\r\n";
    let e = tri::entetes(brut);
    assert!(e.liste_diffusion);
    assert_eq!(e.auto_submitted.as_deref(), Some("auto-generated"));
    assert_eq!(e.precedence.as_deref(), Some("bulk"));
    assert_eq!(e.reply_to.as_deref(), Some("support@exemple.test"));
    assert_eq!(e.message_id.as_deref(), Some("abc@exemple.test"));
    assert_eq!(e.references, vec!["r1@x.test", "r2@x.test"]);
    assert!(!tri::entetes(b"").liste_diffusion);
}

#[test]
fn reponse_de_claude_controlee() {
    let ok = json!({ "structured_output": { "categorie": "question", "resume": "Demande de délai\nligne 2", "priorite": "haute" } });
    let t = tri::depuis_reponse(&ok).unwrap();
    assert_eq!(t.categorie, "question");
    assert_eq!(t.resume, "Demande de délai");
    let texte = json!({ "result": "```json\n{\"categorie\":\"publicite\",\"resume\":\"Promo\",\"priorite\":\"basse\"}\n```" });
    assert_eq!(tri::depuis_reponse(&texte).unwrap().categorie, "publicite");
    let mauvaise =
        json!({ "structured_output": { "categorie": "spam", "resume": "x", "priorite": "basse" } });
    assert!(matches!(
        tri::depuis_reponse(&mauvaise),
        Err(tri::ErreurTri::Schema(_))
    ));
    assert_eq!(
        tri::depuis_reponse(&json!({ "result": "je ne sais pas" })),
        Err(tri::ErreurTri::Absente)
    );
}

#[test]
fn objet_et_fil_de_la_reponse() {
    assert_eq!(reponse::objet_reponse("Commande 12"), "Re: Commande 12");
    assert_eq!(reponse::objet_reponse("RE: Commande 12"), "RE: Commande 12");
    assert_eq!(reponse::objet_reponse("re : x"), "re : x");
    assert_eq!(reponse::objet_reponse(""), "Re:");
    let refs = vec!["a@x.test".to_string(), "<b@x.test>".to_string()];
    assert_eq!(
        reponse::references(&refs, "c@x.test"),
        vec!["a@x.test", "b@x.test", "c@x.test"]
    );
    // Message-ID inventé (mail sans Message-ID) : absent du fil.
    assert_eq!(
        reponse::references(&[], "sha256:abcd"),
        Vec::<String>::new()
    );
    let beaucoup: Vec<String> = (0..30).map(|i| format!("m{i}@x.test")).collect();
    let r = reponse::references(&beaucoup, "fin@x.test");
    assert_eq!(r.len(), 20);
    assert_eq!(r.last().unwrap(), "fin@x.test");
}

#[test]
fn reponse_construite() {
    let refs = vec!["r1@x.test".to_string()];
    let o = Original {
        message_id: "orig@x.test",
        references: &refs,
        expediteur: "jean@exemple.test",
        reply_to: Some("support@exemple.test"),
        objet: "Livraison",
        date: Some(time::macros::datetime!(2026-11-12 10:00 UTC)),
        corps: "Quand livrez-vous ?\n\nJean",
    };
    let r = reponse::construire("factures@frouard.test", Some("MS Frouard"), &o, "Jeudi.").unwrap();
    let brut = String::from_utf8(r.brut).unwrap();
    assert!(brut.contains("To: support@exemple.test"), "{brut}");
    assert!(brut.contains("Subject: Re: Livraison"));
    assert!(brut.contains("In-Reply-To: <orig@x.test>"));
    assert!(brut.contains("References: <r1@x.test> <orig@x.test>"));
    // Corps encodé (accents) : on le relit décodé.
    let lu = mail_parser::MessageParser::default()
        .parse(brut.as_bytes())
        .unwrap();
    let corps = lu.body_text(0).unwrap();
    assert!(corps.starts_with("Jeudi."), "{corps}");
    assert!(corps.contains("Le 12/11/2026, jean@exemple.test a écrit :"));
    assert!(corps.contains("> Quand livrez-vous ?"));
    assert!(r.message_id.starts_with('<') && r.message_id.ends_with("@frouard.test>"));
    assert!(reponse::construire("pas une adresse", None, &o, "x").is_err());
}

#[test]
fn historique_d_un_expediteur() {
    let v = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        tri::par_historique(&v(&["information", "information"])),
        None
    );
    assert_eq!(
        tri::par_historique(&v(&["information", "question", "information"])),
        None
    );
    assert_eq!(
        tri::par_historique(&v(&["publicite", "publicite", "publicite"])).map(|r| r.0),
        Some("publicite".into())
    );
    assert_eq!(tri::par_historique(&v(&["x", "x", "x"])), None);
}

#[test]
fn exemples_dans_le_prompt() {
    assert_eq!(tri::bloc_exemples(&[]), "");
    let b = tri::bloc_exemples(&[tri::Exemple {
        expediteur: "jean@client.test".into(),
        objet: "Livraison".into(),
        corps: "Bonjour,\nPour info, livraison jeudi.\nCordialement,\nJean".into(),
        categorie: "information".into(),
        corrige_de: Some("question".into()),
    }]);
    assert!(b.contains("classés à la main"), "{b}");
    assert!(
        b.contains("Objet : Livraison | Texte : Pour info, livraison jeudi. => information (et non question)"),
        "{b}"
    );
}
