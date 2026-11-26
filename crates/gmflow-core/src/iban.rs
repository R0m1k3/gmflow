//! IBAN du fournisseur à payer (fraude au RIB) : clé mod 97, IBAN connus par fournisseur et
//! alerte « IBAN différent de celui connu » avant dépôt.
//!
//! Un IBAN n'est « connu » qu'après la validation humaine d'une facture qui le porte quand le
//! fournisseur n'en a encore aucun (`retenir_premier`), ou la confirmation explicite d'un
//! administrateur (« C'est le bon IBAN », `confirmer`). Jamais à la simple lecture. Comme le
//! SIRET et la TVA, il est appris au groupe (`groupe_id`, sinon la fiche elle-même) et comparé
//! à ceux de la fiche et de son groupe. Dans les logs, un IBAN n'apparaît que masqué.

use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::controles::Echec;
use crate::journal;

/// Code des alertes « IBAN changé » dans `document.controles`.
pub const CODE_ALERTE: &str = "iban";

/// Longueur de l'IBAN des pays les plus courants (registre SWIFT) ; un autre pays n'est contrôlé
/// que par la longueur générale (15 à 34) et la clé.
const LONGUEURS: &[(&str, usize)] = &[
    ("AD", 24),
    ("AT", 20),
    ("BE", 16),
    ("BG", 22),
    ("CH", 21),
    ("CY", 28),
    ("CZ", 24),
    ("DE", 22),
    ("DK", 18),
    ("EE", 20),
    ("ES", 24),
    ("FI", 18),
    ("FR", 27),
    ("GB", 22),
    ("GR", 27),
    ("HR", 21),
    ("HU", 28),
    ("IE", 22),
    ("IT", 27),
    ("LI", 21),
    ("LT", 20),
    ("LU", 20),
    ("LV", 21),
    ("MC", 27),
    ("MT", 31),
    ("NL", 18),
    ("NO", 15),
    ("PL", 28),
    ("PT", 25),
    ("RO", 24),
    ("SE", 24),
    ("SI", 19),
    ("SK", 24),
    ("SM", 27),
    ("TR", 26),
];

const PAYS: &[(&str, &str)] = &[
    ("AD", "Andorre"),
    ("AT", "Autriche"),
    ("BE", "Belgique"),
    ("BG", "Bulgarie"),
    ("CH", "Suisse"),
    ("CY", "Chypre"),
    ("CZ", "Tchéquie"),
    ("DE", "Allemagne"),
    ("DK", "Danemark"),
    ("EE", "Estonie"),
    ("ES", "Espagne"),
    ("FI", "Finlande"),
    ("FR", "France"),
    ("GB", "Royaume-Uni"),
    ("GR", "Grèce"),
    ("HR", "Croatie"),
    ("HU", "Hongrie"),
    ("IE", "Irlande"),
    ("IT", "Italie"),
    ("LI", "Liechtenstein"),
    ("LT", "Lituanie"),
    ("LU", "Luxembourg"),
    ("LV", "Lettonie"),
    ("MC", "Monaco"),
    ("MT", "Malte"),
    ("NL", "Pays-Bas"),
    ("NO", "Norvège"),
    ("PL", "Pologne"),
    ("PT", "Portugal"),
    ("RO", "Roumanie"),
    ("SE", "Suède"),
    ("SI", "Slovénie"),
    ("SK", "Slovaquie"),
    ("SM", "Saint-Marin"),
    ("TR", "Turquie"),
];

/// IBAN en majuscules, sans espaces ni ponctuation (« fr76 3000 … » -> « FR763000… »).
pub fn compacte(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// IBAN au bon format (pays, longueur) et dont la clé mod 97 est juste.
pub fn valide(s: &str) -> bool {
    let c = compacte(s);
    let b = c.as_bytes();
    if !(15..=34).contains(&c.len())
        || !b[..2].iter().all(u8::is_ascii_uppercase)
        || !b[2..4].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    if let Some((_, n)) = LONGUEURS.iter().find(|(p, _)| *p == &c[..2])
        && *n != c.len()
    {
        return false;
    }
    // Les 4 premiers caractères passent à la fin, chaque lettre devient deux chiffres
    // (A = 10 … Z = 35) : le nombre obtenu modulo 97 vaut 1.
    let mut reste: u32 = 0;
    for ch in c[4..].chars().chain(c[..4].chars()) {
        let v = match ch.to_digit(36) {
            Some(v) => v,
            None => return false,
        };
        reste = if v >= 10 {
            (reste * 100 + v) % 97
        } else {
            (reste * 10 + v) % 97
        };
    }
    reste == 1
}

/// BIC (SWIFT) au bon format : 4 lettres (banque), 2 lettres (pays), 2 caractères (lieu), puis
/// 3 facultatifs (agence).
pub fn bic_valide(s: &str) -> bool {
    let c = compacte(s);
    let b = c.as_bytes();
    matches!(c.len(), 8 | 11) && b[..6].iter().all(u8::is_ascii_uppercase)
}

/// IBAN masqué au milieu : « FR76…1234 ».
pub fn masque(s: &str) -> String {
    let c = compacte(s);
    if c.len() <= 8 {
        return c;
    }
    format!("{}…{}", &c[..4], &c[c.len() - 4..])
}

/// Code pays de l'IBAN (« FR »).
pub fn pays(s: &str) -> String {
    compacte(s).chars().take(2).collect()
}

/// Nom du pays en français (le code s'il n'est pas dans la liste).
pub fn nom_pays(code: &str) -> String {
    PAYS.iter()
        .find(|(c, _)| *c == code)
        .map_or_else(|| code.to_string(), |(_, n)| n.to_string())
}

/// IBAN et BIC gardés d'une lecture : l'IBAN seulement si sa clé est juste ; le BIC seulement
/// avec un IBAN gardé, et s'il est au bon format.
pub fn nettoyer(iban: Option<&str>, bic: Option<&str>) -> (Option<String>, Option<String>) {
    let iban = iban.map(compacte).filter(|i| valide(i));
    let bic = iban
        .as_ref()
        .and(bic)
        .map(compacte)
        .filter(|b| bic_valide(b));
    (iban, bic)
}

/// Anomalie de lecture d'un IBAN lu dont la clé est fausse (rien s'il est absent ou juste).
pub fn anomalie(iban_lu: Option<&str>) -> Option<String> {
    let i = iban_lu.map(str::trim).filter(|i| !i.is_empty())?;
    (!valide(i)).then(|| {
        format!(
            "l'IBAN lu « {} » n'a pas une clé valide (chiffre mal lu ?)",
            masque(i)
        )
    })
}

/// Alerte « IBAN changé » : l'IBAN de la facture n'est pas parmi ceux connus pour ce
/// fournisseur, qui en a déjà au moins un. Rien pour un premier IBAN ou sans IBAN lu.
pub fn controler(lu: Option<&str>, connus: &[String]) -> Option<Echec> {
    let lu = compacte(lu?);
    if lu.is_empty() || connus.is_empty() || connus.iter().any(|c| compacte(c) == lu) {
        return None;
    }
    let liste: Vec<String> = connus.iter().take(3).map(|c| masque(c)).collect();
    let (de, liste) = if connus.len() > 1 {
        ("de ceux connus", liste.join(" ou "))
    } else {
        ("de celui connu", liste.join(""))
    };
    let p = pays(&lu);
    let mut habituels: Vec<String> = connus.iter().map(|c| pays(c)).collect();
    habituels.sort();
    habituels.dedup();
    let autre_pays = if habituels.contains(&p) {
        String::new()
    } else {
        format!(
            ", et dans un autre pays : {} au lieu de {}",
            nom_pays(&p),
            habituels
                .iter()
                .map(|h| nom_pays(h))
                .collect::<Vec<_>>()
                .join(" ou ")
        )
    };
    Some(Echec {
        code: CODE_ALERTE.into(),
        message: format!(
            "IBAN différent {de} pour ce fournisseur ({} au lieu de {liste}){autre_pays}. \
             Vérifiez par téléphone auprès du fournisseur avant de payer",
            masque(&lu)
        ),
        champs: Vec::new(),
        proposition: None,
    })
}

/// Après une lecture : IBAN et BIC du document repris de `resultat` (réponse de Claude ou XML),
/// vides si la clé de l'IBAN est fausse.
pub async fn noter_lecture(conn: &mut PgConnection, document_id: i64) -> sqlx::Result<()> {
    let Some(r) = sqlx::query!(
        "select resultat->>'iban' as iban, resultat->>'bic' as bic from document where id = $1",
        document_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    let (iban, bic) = nettoyer(r.iban.as_deref(), r.bic.as_deref());
    if r.iban.is_some() && iban.is_none() {
        tracing::info!(
            document = document_id,
            iban = %masque(r.iban.as_deref().unwrap_or_default()),
            "IBAN lu à la clé fausse : non gardé"
        );
    }
    sqlx::query!(
        "update document set iban = $2, bic = $3 where id = $1",
        document_id,
        iban,
        bic
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Fiche où un IBAN est appris : le groupe du fournisseur, sinon la fiche elle-même.
async fn fiche_d_apprentissage(conn: &mut PgConnection, fournisseur_id: i64) -> sqlx::Result<i64> {
    Ok(sqlx::query_scalar!(
        "select coalesce(groupe_id, id) as \"id!\" from fournisseur where id = $1",
        fournisseur_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(fournisseur_id))
}

/// IBAN connus pour un fournisseur : ceux de sa fiche et de son groupe, du plus récent au plus
/// ancien.
pub async fn connus(conn: &mut PgConnection, fournisseur_id: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        "select i.iban from fournisseur_iban i
         where i.fournisseur_id = $1
            or i.fournisseur_id = (select groupe_id from fournisseur where id = $1)
         order by i.ajoute_le desc, i.id desc",
        fournisseur_id
    )
    .fetch_all(&mut *conn)
    .await
}

/// Alerte « IBAN changé » d'un document (contrôle avant dépôt).
pub async fn alerte(conn: &mut PgConnection, document_id: i64) -> sqlx::Result<Option<Echec>> {
    let Some(d) = sqlx::query!(
        "select iban, fournisseur_id from document where id = $1",
        document_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let (Some(iban), Some(f)) = (d.iban, d.fournisseur_id) else {
        return Ok(None);
    };
    let connus = connus(conn, f).await?;
    Ok(controler(Some(&iban), &connus))
}

/// Enregistre un IBAN connu (sans effet s'il l'est déjà) et le journalise. Rend vrai s'il est
/// nouveau.
async fn ajouter(
    conn: &mut PgConnection,
    document_id: i64,
    auteur_id: Option<i64>,
    origine: &str,
) -> sqlx::Result<bool> {
    let Some(d) = sqlx::query!(
        "select iban, bic, fournisseur_id, date_document from document where id = $1",
        document_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };
    let (Some(iban), Some(f)) = (d.iban, d.fournisseur_id) else {
        return Ok(false);
    };
    let fiche = fiche_d_apprentissage(conn, f).await?;
    let ajoute = sqlx::query!(
        "insert into fournisseur_iban (fournisseur_id, iban, bic, origine, document_id,
                                       premiere_vue_le, ajoute_par)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (fournisseur_id, iban) do nothing",
        fiche,
        iban,
        d.bic,
        origine,
        document_id,
        d.date_document,
        auteur_id
    )
    .execute(&mut *conn)
    .await?
    .rows_affected()
        > 0;
    if ajoute {
        journal::ecrire(
            &mut *conn,
            auteur_id,
            if origine == "confirmation" {
                "iban_confirme"
            } else {
                "iban_ajoute"
            },
            "fournisseur",
            Some(fiche.to_string()),
            json!({ "iban": masque(&iban), "document_id": document_id, "origine": origine }),
        )
        .await?;
    }
    Ok(ajoute)
}

/// Facture vérifiée par un humain : son IBAN devient connu si le fournisseur n'en a encore
/// aucun (premier IBAN, sans alerte). Un IBAN différent de ceux connus ne l'est jamais ainsi :
/// seule `confirmer` l'ajoute. Rend vrai si un IBAN a été ajouté.
pub async fn retenir_premier(conn: &mut PgConnection, document_id: i64) -> sqlx::Result<bool> {
    let Some(d) = sqlx::query!(
        "select iban, fournisseur_id, verifie_par from document
         where id = $1 and verifie_le is not null",
        document_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(false);
    };
    let (Some(_), Some(f)) = (d.iban, d.fournisseur_id) else {
        return Ok(false);
    };
    if !connus(conn, f).await?.is_empty() {
        return Ok(false);
    }
    ajouter(conn, document_id, d.verifie_par, "validation").await
}

/// « C'est le bon IBAN » : l'IBAN du document devient connu pour son fournisseur, par cet
/// administrateur (journalisé). Rend vrai s'il ne l'était pas encore ; `None` si le document n'a
/// pas d'IBAN ou pas de fournisseur.
pub async fn confirmer(
    conn: &mut PgConnection,
    document_id: i64,
    auteur_id: i64,
) -> sqlx::Result<Option<bool>> {
    let d = sqlx::query!(
        "select iban, fournisseur_id from document where id = $1 for update",
        document_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    match d {
        Some(d) if d.iban.is_some() && d.fournisseur_id.is_some() => {
            let ajoute = ajouter(conn, document_id, Some(auteur_id), "confirmation").await?;
            if !ajoute {
                journal::ecrire(
                    &mut *conn,
                    Some(auteur_id),
                    "iban_confirme",
                    "document",
                    Some(document_id.to_string()),
                    json!({ "deja_connu": true }),
                )
                .await?;
            }
            Ok(Some(ajoute))
        }
        _ => Ok(None),
    }
}

/// Retire l'alerte « IBAN changé » des contrôles enregistrés (document parti : ses contrôles
/// ne sont plus recalculés).
pub fn sans_alerte(controles: &Value) -> Value {
    match controles.as_array() {
        Some(v) => Value::Array(
            v.iter()
                .filter(|c| c["code"] != CODE_ALERTE)
                .cloned()
                .collect(),
        ),
        None => controles.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // IBAN d'exemple publiés (clés justes).
    const FR: &str = "FR14 2004 1010 0505 0001 3M02 606";
    const FR2: &str = "FR7630006000011234567890189";
    const DE: &str = "DE89 3704 0044 0532 0130 00";
    const LT: &str = "LT12 1000 0111 0100 1000";

    #[test]
    fn cle_mod_97() {
        for i in [
            FR,
            FR2,
            DE,
            LT,
            "GB82WEST12345698765432",
            "BE68539007547034",
        ] {
            assert!(valide(i), "{i}");
        }
        // Un chiffre changé, deux chiffres inversés, mauvaise longueur, pas un IBAN.
        assert!(!valide("FR7630006000011234567890188"));
        assert!(!valide("FR7630006000011234567809189"));
        assert!(!valide("FR763000600001123456789018"));
        assert!(!valide("DE89370400440532013001"));
        assert!(!valide("1234567890123456"));
        assert!(!valide(""));
        assert_eq!(compacte(" fr76 3000-6000 "), "FR7630006000");
    }

    #[test]
    fn masque_et_nettoyage() {
        assert_eq!(masque(FR2), "FR76…0189");
        assert_eq!(pays(DE), "DE");
        assert_eq!(nom_pays("LT"), "Lituanie");
        assert_eq!(nom_pays("XX"), "XX");
        assert_eq!(
            nettoyer(
                Some("fr76 3000 6000 0112 3456 7890 189"),
                Some("agri frpp xxx")
            ),
            (Some(FR2.to_string()), Some("AGRIFRPPXXX".to_string()))
        );
        // Clé fausse : rien de gardé, ni le BIC ; anomalie de lecture.
        assert_eq!(
            nettoyer(Some("FR7630006000011234567890188"), Some("AGRIFRPP")),
            (None, None)
        );
        assert!(anomalie(Some("FR7630006000011234567890188")).is_some());
        assert!(anomalie(Some(FR2)).is_none());
        assert!(anomalie(None).is_none());
        assert_eq!(nettoyer(Some(FR2), Some("12")).1, None);
    }

    #[test]
    fn premier_iban_sans_alerte_puis_iban_change() {
        assert!(controler(Some(FR2), &[]).is_none());
        assert!(controler(None, &[compacte(FR)]).is_none());
        assert!(controler(Some(FR2), &[compacte(FR2)]).is_none());
        // Écrit autrement (espaces, minuscules) : le même.
        assert!(controler(Some("fr76 3000 6000 0112 3456 7890 189"), &[compacte(FR2)]).is_none());
        let e = controler(Some(FR2), &[compacte(FR)]).unwrap();
        assert_eq!(e.code, "iban");
        assert_eq!(
            e.message,
            "IBAN différent de celui connu pour ce fournisseur (FR76…0189 au lieu de FR14…2606). \
             Vérifiez par téléphone auprès du fournisseur avant de payer"
        );
    }

    #[test]
    fn autre_pays_mentionne() {
        let e = controler(Some(LT), &[compacte(FR), compacte(FR2)]).unwrap();
        assert!(
            e.message.starts_with(
                "IBAN différent de ceux connus pour ce fournisseur (LT12…1000 au lieu de \
                 FR14…2606 ou FR76…0189), et dans un autre pays : Lituanie au lieu de France."
            ),
            "{}",
            e.message
        );
    }

    #[test]
    fn alerte_retiree_des_controles() {
        let c = json!([{ "code": "iban", "message": "x" }, { "code": "montants", "message": "y" }]);
        assert_eq!(
            sans_alerte(&c),
            json!([{ "code": "montants", "message": "y" }])
        );
    }
}
