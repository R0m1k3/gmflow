//! Test de connexion d'une boîte : LOGIN puis LIST ; le résultat va dans `dernier_test`.

use gmflow_core::imap;
use serde_json::json;

use crate::Contexte;

pub async fn tester(ctx: &Contexte, boite_id: i64) -> anyhow::Result<()> {
    let b = sqlx::query!(
        "select serveur, port, identifiant, mot_de_passe_chiffre from boite_mail where id = $1",
        boite_id
    )
    .fetch_optional(&ctx.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("boîte {boite_id} introuvable"))?;

    let resultat = async {
        let mdp = ctx
            .chiffreur
            .dechiffrer_texte(&b.mot_de_passe_chiffre)
            .map_err(|e| e.to_string())?;
        let port = u16::try_from(b.port).map_err(|_| "port invalide".to_string())?;
        let mut s = imap::connecter(&b.serveur, port, &b.identifiant, &mdp, ctx.transport)
            .await
            .map_err(|e| e.to_string())?;
        let dossiers = imap::dossiers(&mut s).await.map_err(|e| e.to_string())?;
        let _ = s.logout().await;
        Ok::<_, String>(dossiers)
    }
    .await;

    let quand =
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    let test = match &resultat {
        Ok(d) => {
            json!({ "quand": quand, "ok": true, "message": format!("{} dossiers", d.len()), "dossiers": d })
        }
        Err(m) => json!({ "quand": quand, "ok": false, "message": m, "dossiers": [] }),
    };
    sqlx::query!(
        "update boite_mail set dernier_test = $2 where id = $1",
        boite_id,
        test
    )
    .execute(&ctx.pool)
    .await?;
    Ok(())
}

/// Essai demandé par l'assistant d'ajout ou la fiche d'une boîte (module `essai_boite`) :
/// IMAP (LOGIN, LIST, dossiers de réception et des envoyés) et/ou SMTP (connexion et
/// authentification, sans envoi). Le mot de passe est effacé de la demande à la fin.
pub async fn essayer(ctx: &Contexte, essai_id: uuid::Uuid) -> anyhow::Result<()> {
    use gmflow_core::essai_boite::{self, Essai, ResultatImap, ResultatSmtp};
    use gmflow_core::{expedition, parametre};

    let cle = essai_boite::cle(essai_id);
    let Some(v) = parametre::lire(&ctx.pool, &cle).await? else {
        return Ok(());
    };
    let mut essai: Essai = serde_json::from_value(v)?;
    if essai.etat != "en_cours" {
        return Ok(());
    }
    let mdp = match parametre::lire_secret(&ctx.pool, &cle).await? {
        Some(c) => ctx
            .chiffreur
            .dechiffrer_texte(&c)
            .map_err(|e| e.to_string()),
        None => Err("mot de passe absent".to_string()),
    };

    if let Some(p) = &essai.imap {
        essai.resultat_imap = Some(match &mdp {
            Err(m) => ResultatImap {
                ok: false,
                message: format!("Essai impossible : {m}."),
                detail: None,
                dossiers: Vec::new(),
                reception: None,
                envoyes: None,
            },
            Ok(mdp) => {
                let r = async {
                    let mut s =
                        imap::connecter(&p.serveur, p.port, &essai.identifiant, mdp, ctx.transport)
                            .await?;
                    let d = imap::dossiers_detail(&mut s).await;
                    let _ = s.logout().await;
                    d
                }
                .await;
                match r {
                    Ok(dossiers) => {
                        let reception = dossiers
                            .iter()
                            .find(|d| d.nom.eq_ignore_ascii_case("INBOX"))
                            .map(|d| d.nom.clone());
                        let envoyes = imap::dossier_envoyes(&dossiers).map(|d| d.nom.clone());
                        ResultatImap {
                            ok: true,
                            message: format!(
                                "Connexion réussie : {} dossiers trouvés.",
                                dossiers.len()
                            ),
                            detail: None,
                            dossiers,
                            reception,
                            envoyes,
                        }
                    }
                    Err(e) => {
                        let (message, detail) = essai_boite::message_imap(&e);
                        ResultatImap {
                            ok: false,
                            message,
                            detail: Some(detail),
                            dossiers: Vec::new(),
                            reception: None,
                            envoyes: None,
                        }
                    }
                }
            }
        });
    }

    if let Some(p) = &essai.smtp {
        essai.resultat_smtp = Some(match &mdp {
            Err(m) => ResultatSmtp {
                ok: false,
                message: format!("Essai impossible : {m}."),
                detail: None,
            },
            Ok(mdp) => {
                let config = expedition::Expedition {
                    serveur: p.serveur.clone(),
                    port: p.port,
                    securite: p.securite.clone(),
                    identifiant: essai.identifiant.clone(),
                    adresse: essai.identifiant.clone(),
                    nom: None,
                };
                match expedition::tester_connexion(&config, Some(mdp)).await {
                    Ok(()) => ResultatSmtp {
                        ok: true,
                        message:
                            "Envoi des réponses possible : le serveur SMTP accepte la connexion."
                                .into(),
                        detail: None,
                    },
                    Err(e) => {
                        let (message, detail) = essai_boite::message_smtp(&e);
                        ResultatSmtp {
                            ok: false,
                            message,
                            detail: Some(detail),
                        }
                    }
                }
            }
        });
    }

    essai.etat = "termine".into();
    essai.termine_le = Some(
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?,
    );
    let mut tx = ctx.pool.begin().await?;
    parametre::ecrire(&mut *tx, &cle, &serde_json::to_value(&essai)?).await?;
    sqlx::query!(
        "update parametre set valeur_chiffree = null where cle = $1",
        cle
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
