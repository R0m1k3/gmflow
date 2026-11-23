//! Réglages > Traitements : état de chaque étape, relance demandée au worker par la base,
//! « Tout relire », raison « lecture à l'arrêt » sur la fiche d'un document.

mod commun;

use axum::http::StatusCode;
use commun::{Appli, fixture};
use serde_json::Value;

const MDP: &str = "un mot de passe solide";

async fn executer(app: &Appli, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(app.pool())
        .await
        .unwrap();
}

async fn document(app: &Appli, mail_id: i64, sql_etat: &str) -> i64 {
    let piece = app.piece_de(mail_id).await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, voie, type, fournisseur_nom)
         select id, mail_id, sha256, 'pdf_texte', 'facture', 'Durand'
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .fetch_one(app.pool())
    .await
    .unwrap();
    executer(
        app,
        &format!("update document set {sql_etat} where id = {id}"),
    )
    .await;
    id
}

fn etape<'a>(v: &'a Value, cle: &str) -> &'a Value {
    v["etapes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["cle"] == cle)
        .unwrap()
}

#[tokio::test]
async fn traitements_relances_et_tout_relire() {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let boite = app.boite(sa, "Compta Frouard").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    let admin = app.connecter("admin", MDP).await;
    let resp = app.connecter("resp-a", MDP).await;
    let admin = Some(admin.as_str());
    let m1 = app.mail_vu(&fixture("pdf.eml"), &[boite]).await;
    let m2 = app.mail_vu(&fixture("inline.eml"), &[boite]).await;
    let vieux = document(
        &app,
        m1,
        "etat = 'a_lire', cree_le = now() - interval '1 hour'",
    )
    .await;
    let pret = document(
        &app,
        m2,
        &format!(
            "etat = 'lu', lu_le = now(), statut_depot = 'pret', valide_le = now(), societe_id = {sa}"
        ),
    )
    .await;

    assert_eq!(
        app.requete("GET", "/api/admin/traitements", None, Some(&resp))
            .await
            .code,
        StatusCode::FORBIDDEN
    );
    // Aucun worker n'a jamais tourné : tout est à l'arrêt.
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let cles: Vec<&str> = r.json["etapes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["cle"].as_str().unwrap())
        .collect();
    assert_eq!(cles, ["releve", "lecture", "tri", "rapprochement", "depot"]);
    assert_eq!(r.json["service"]["en_marche"], false);
    assert_eq!(etape(&r.json, "lecture")["etat"], "arrete");

    // Le worker tourne : les files battent, l'orchestre a écrit son état.
    for f in gmflow_core::traitements::FILES {
        executer(
            &app,
            &format!(
                "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
                 values ('gmflow-{f}', '{f}', 'PgStorage', '', now())"
            ),
        )
        .await;
    }
    executer(
        &app,
        "insert into parametre (cle, valeur)
         values ('orchestre_etat', jsonb_build_object('vu_le', to_char(now() at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')))",
    )
    .await;
    executer(
        &app,
        &format!("update boite_mail set active = true where id = {boite}"),
    )
    .await;
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    assert_eq!(r.json["service"]["en_marche"], true, "{}", r.json);
    let lecture = etape(&r.json, "lecture");
    // Claude pas connecté et un document attend : bloquée, pas « normale » ni « en retard ».
    assert_eq!(lecture["etat"], "bloque", "{lecture}");
    assert!(
        lecture["blocage"]
            .as_str()
            .unwrap()
            .contains("pas connecté"),
        "{lecture}"
    );
    assert_eq!(lecture["en_attente"], 1);
    assert!(
        lecture["plus_vieille"]["raison"]
            .as_str()
            .unwrap()
            .contains("pas connecté"),
        "{lecture}"
    );
    assert!(r.json["claude"].as_str().unwrap().contains("pas connecté"));
    let depot = etape(&r.json, "depot");
    assert_eq!(depot["en_attente"], 1, "{depot}");
    assert_eq!(depot["file"]["vivante"], true);
    // Adresse d'envoi pas réglée et une facture attend son dépôt : bloqué.
    assert_eq!(depot["etat"], "bloque", "{depot}");
    assert!(
        depot["blocage"]
            .as_str()
            .unwrap()
            .contains("adresse d’envoi")
    );
    // Rien à trier : le tri n'est pas bloqué, même sans Claude.
    assert_eq!(etape(&r.json, "tri")["etat"], "ok");
    assert_eq!(etape(&r.json, "tri")["blocage"], Value::Null);
    assert_eq!(
        etape(&r.json, "releve")["etat"],
        "en_retard",
        "boîte jamais relevée"
    );
    // Un mail attend son tri sans Claude : bloqué. Adresse d'envoi réglée mais société sans
    // adresse de collecte : toujours bloqué, pour une autre raison.
    executer(
        &app,
        &format!(
            "update mail set tri_etat = 'a_trier' where id = {m1};
             insert into parametre (cle, valeur) values ('smtp_expedition', '{{\"serveur\": \"smtp.test\"}}')"
        ),
    )
    .await;
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    let tri = etape(&r.json, "tri");
    assert_eq!(tri["etat"], "bloque", "{tri}");
    assert!(tri["blocage"].as_str().unwrap().contains("pas connecté"));
    let depot = etape(&r.json, "depot");
    assert_eq!(depot["etat"], "bloque", "{depot}");
    assert!(
        depot["blocage"]
            .as_str()
            .unwrap()
            .contains("adresse de collecte")
    );
    // Adresse de collecte indiquée : le dépôt peut partir.
    executer(
        &app,
        &format!("update societe set adresse_depot = 'collecte@dext.test' where id = {sa}"),
    )
    .await;
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    let depot = etape(&r.json, "depot");
    assert_eq!(depot["etat"], "ok", "{depot}");
    assert_eq!(depot["blocage"], Value::Null);
    executer(
        &app,
        &format!("update mail set tri_etat = null where id = {m1}"),
    )
    .await;

    // Relance : une demande déposée dans la base, visible jusqu'à ce que le worker la prenne.
    let r = app
        .requete("POST", "/api/admin/traitements/depot/relancer", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    assert!(r.json["demandee_le"].is_string());
    assert_eq!(
        app.requete("POST", "/api/admin/traitements/autre/relancer", None, admin)
            .await
            .code,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.requete(
            "POST",
            "/api/admin/traitements/depot/relancer",
            None,
            Some(&resp)
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    assert_eq!(etape(&r.json, "depot")["relance_demandee"], true);
    assert_eq!(etape(&r.json, "lecture")["relance_demandee"], false);
    assert_eq!(
        gmflow_core::traitements::prendre_demandes(app.pool())
            .await
            .unwrap(),
        ["depot"]
    );
    assert_eq!(
        gmflow_core::traitements::documents_a_deposer(app.pool())
            .await
            .unwrap(),
        [pret]
    );

    // Tout relire : le document en attente part en lecture une fois, pas deux.
    let r = app
        .requete("POST", "/api/admin/traitements/tout-relire", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    assert_eq!(r.json["documents"], 1);
    assert_eq!(
        gmflow_core::file::documents_en_file(app.pool())
            .await
            .unwrap(),
        [vieux]
    );
    let r = app
        .requete("POST", "/api/admin/traitements/tout-relire", None, admin)
        .await;
    assert_eq!(r.json["documents"], 0);

    // La file de lecture ne bat plus : la fiche du document le dit.
    executer(
        &app,
        "update apalis.workers set last_seen = now() - interval '10 minutes' where worker_type = 'lecture'",
    )
    .await;
    let r = app
        .requete("GET", &format!("/api/documents/{vieux}"), None, admin)
        .await;
    assert_eq!(
        r.json["attente"]["genre"], "file_arretee",
        "{}",
        r.json["attente"]
    );
    assert!(
        r.json["attente"]["raison"]
            .as_str()
            .unwrap()
            .starts_with("La lecture est à l’arrêt depuis")
    );
    let r = app
        .requete("GET", "/api/admin/traitements", None, admin)
        .await;
    assert_eq!(etape(&r.json, "lecture")["etat"], "arrete");
    assert_eq!(etape(&r.json, "rapprochement")["etat"], "arrete");
    assert_eq!(etape(&r.json, "tri")["etat"], "ok");
    assert_eq!(
        app.compter("select count(*) from journal where action in ('traitement_relance_demandee', 'tout_relire')")
            .await,
        3
    );
    app.fermer().await;
}
