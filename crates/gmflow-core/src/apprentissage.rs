//! Ce que la lecture apprend des pièces validées d'un fournisseur (blocs de prompt) : un exemple
//! positif (sa dernière pièce validée, même type, en JSON compact et borné) et le résumé des
//! champs souvent corrigés à la main. Fonctions pures ; les requêtes sont dans le worker
//! (`fournisseur::exemples`).

use serde_json::{Map, Value, json};

/// Taille maximale du JSON de l'exemple positif, en caractères (environ 400 tokens) : au-delà,
/// les lignes sont retirées une à une, puis l'exemple n'est pas donné.
pub const EXEMPLE_CARACTERES_MAX: usize = 1200;
/// Lignes articles montrées dans l'exemple.
pub const EXEMPLE_LIGNES: usize = 3;
/// Longueur maximale d'un texte de l'exemple (désignation…).
const TEXTE_MAX: usize = 60;
/// Pièces validées regardées pour le résumé des corrections.
pub const CORRECTIONS_SUR: i64 = 20;
/// Champs montrés dans le résumé des corrections (les plus corrigés).
const CHAMPS_MAX: usize = 6;

/// Nom d'un champ corrigé (nom de l'API, clé de `document.corrections`) dans le schéma de lecture.
pub fn champ_du_schema(champ: &str) -> &str {
    match champ {
        "date_document" => "date",
        "date_echeance" => "echeance",
        autre => autre,
    }
}

fn nom_type(t: &str) -> &'static str {
    match t {
        "avoir" => "avoir",
        "bl" => "bon de livraison",
        _ => "facture",
    }
}

fn sans_accent(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => 'A',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'î' | 'ï' | 'Î' | 'Ï' => 'I',
        'ô' | 'ö' | 'Ô' | 'Ö' => 'O',
        'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' | 'Ç' => 'C',
        c => c.to_ascii_uppercase(),
    }
}

/// Type probable d'une pièce avant sa lecture, pour choisir l'exemple : le premier intitulé
/// trouvé dans le début du texte (« FACTURE », « AVOIR N° », « BON DE LIVRAISON »…), sinon dans
/// le nom du fichier puis l'objet du mail. Rien si aucun ne parle (scan sans nom parlant).
pub fn type_probable(texte: Option<&str>, nom_fichier: &str, objet: &str) -> Option<&'static str> {
    let motifs = regex::Regex::new(
        r"\b(?:(FACTURE D.?AVOIR|AVOIR\s*(?:N\b|NO\b|N°|NUM|:|\d)|NOTE DE CREDIT)|(BON DE LIVRAISON|BORDEREAU DE LIVRAISON|BON LIVRAISON|BL\s*(?:N\b|NO\b|N°|:|\d))|(FACTURE))",
    )
    .ok()?;
    let chercher = |s: &str| -> Option<&'static str> {
        let s: String = s.chars().take(3000).map(sans_accent).collect();
        let c = motifs.captures(&s)?;
        if c.get(1).is_some() {
            Some("avoir")
        } else if c.get(2).is_some() {
            Some("bl")
        } else {
            Some("facture")
        }
    };
    texte
        .and_then(chercher)
        .or_else(|| chercher(&nom_fichier.replace(['_', '-', '.'], " ")))
        .or_else(|| chercher(objet))
}

/// Pièce validée montrée en exemple : en-tête tel que validé (noms du schéma de lecture) et
/// premières lignes.
#[derive(Debug, Clone, Default)]
pub struct PieceValidee {
    pub r#type: String,
    pub entete: Map<String, Value>,
    pub lignes: Vec<Map<String, Value>>,
}

fn net(v: &Value) -> Option<Value> {
    match v {
        Value::Null => None,
        Value::String(s) if s.trim().is_empty() => None,
        Value::String(s) => Some(Value::String(s.chars().take(TEXTE_MAX).collect())),
        Value::Array(a) if a.is_empty() => None,
        Value::Object(o) => {
            let o: Map<String, Value> = o
                .iter()
                .filter_map(|(k, v)| net(v).map(|v| (k.clone(), v)))
                .collect();
            (!o.is_empty()).then_some(Value::Object(o))
        }
        v => Some(v.clone()),
    }
}

/// JSON compact de l'exemple (champs vides retirés, textes coupés), dans la borne de taille :
/// sans lignes au besoin, rien s'il ne tient toujours pas.
pub fn json_exemple(p: &PieceValidee) -> Option<String> {
    let mut entete = net(&Value::Object(p.entete.clone()))
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    entete.insert("type".into(), json!(p.r#type));
    let lignes: Vec<Value> = p
        .lignes
        .iter()
        .take(EXEMPLE_LIGNES)
        .filter_map(|l| net(&Value::Object(l.clone())))
        .collect();
    for n in (0..=lignes.len()).rev() {
        let mut v = entete.clone();
        if n > 0 {
            v.insert("lignes".into(), Value::Array(lignes[..n].to_vec()));
        }
        let s = Value::Object(v).to_string();
        if s.chars().count() <= EXEMPLE_CARACTERES_MAX {
            return Some(s);
        }
    }
    None
}

/// Bloc de prompt de l'exemple positif. `meme_fiche` : faux si l'exemple vient d'une autre
/// marque du même groupe.
pub fn bloc_exemple(p: &PieceValidee, meme_fiche: bool) -> Option<String> {
    let j = json_exemple(p)?;
    let de = if meme_fiche {
        "ce fournisseur"
    } else {
        "une autre marque du même groupe que ce fournisseur"
    };
    Some(format!(
        "\nExemple de la forme attendue : la dernière pièce de {de} validée par un humain ({}), \
         en JSON (en-tête tel que validé, et ses {} premières lignes au plus). Il montre où ce \
         fournisseur imprime chaque information et comment la recopier. Ce n'est pas le \
         document à lire : toutes les valeurs de ta réponse viennent du document courant, ne \
         recopie rien de cet exemple.\n{j}\n",
        nom_type(&p.r#type),
        EXEMPLE_LIGNES
    ))
}

fn court(v: &Value) -> String {
    let s = match v {
        Value::Null => "vide".to_string(),
        Value::String(s) => format!("« {} »", s.chars().take(40).collect::<String>()),
        autre => autre.to_string(),
    };
    s.chars().take(46).collect()
}

/// Résumé des corrections faites à la main sur les dernières pièces validées du fournisseur
/// (`corrections` de chaque pièce, de la plus récente à la plus ancienne) : par champ, combien de
/// fois corrigé sur combien de pièces, avec le dernier couple lu → corrigé. Le fournisseur
/// corrigé n'y est pas (Claude ne le choisit pas). Vide sans correction.
pub fn bloc_corrections(corrections: &[Value]) -> String {
    // (champ, nombre, dernier avant, dernier après), dans l'ordre de première rencontre.
    let mut champs: Vec<(String, usize, Value, Value)> = Vec::new();
    for c in corrections {
        let Some(o) = c.as_object() else { continue };
        for (champ, v) in o {
            if champ == "fournisseur_id" {
                continue;
            }
            match champs.iter_mut().find(|(n, ..)| n == champ) {
                Some(x) => x.1 += 1,
                None => champs.push((champ.clone(), 1, v["avant"].clone(), v["apres"].clone())),
            }
        }
    }
    if champs.is_empty() {
        return String::new();
    }
    champs.sort_by_key(|c| std::cmp::Reverse(c.1));
    let n = corrections.len();
    let mut s = format!(
        "\nChamps souvent mal lus sur ce fournisseur (corrigés à la main sur ses {n} dernières \
         pièces validées) : vérifie-les avec soin sur ce document.\n"
    );
    for (champ, k, avant, apres) in champs.iter().take(CHAMPS_MAX) {
        s.push_str(&format!(
            "- {} : corrigé {k} fois sur {n} (dernière fois : lu {}, corrigé en {})\n",
            champ_du_schema(champ),
            court(avant),
            court(apres)
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(lignes: usize, designation: &str) -> PieceValidee {
        let mut entete = Map::new();
        entete.insert("numero".into(), json!("F-12"));
        entete.insert("date".into(), json!("2026-11-02"));
        entete.insert("echeance".into(), Value::Null);
        entete.insert("montant_ttc".into(), json!(120.5));
        entete.insert(
            "fournisseur".into(),
            json!({ "nom": "Durand", "siret": null, "tva": "FR96552100554" }),
        );
        PieceValidee {
            r#type: "facture".into(),
            entete,
            lignes: (0..lignes)
                .map(|i| {
                    let mut l = Map::new();
                    l.insert("reference".into(), json!(format!("R{i}")));
                    l.insert("designation".into(), json!(designation));
                    l.insert("quantite".into(), json!(2));
                    l.insert("gencod".into(), Value::Null);
                    l
                })
                .collect(),
        }
    }

    #[test]
    fn exemple_compact_et_borne() {
        let j = json_exemple(&piece(5, "Bougie")).unwrap();
        let v: Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["lignes"].as_array().unwrap().len(), 3);
        assert!(!j.contains("null") && !j.contains("echeance") && !j.contains("siret"));
        assert_eq!(v["fournisseur"]["tva"], "FR96552100554");
        assert_eq!(v["type"], "facture");
        // Désignations longues : coupées à 60 caractères, puis lignes retirées pour tenir.
        let long = "x".repeat(500);
        let j = json_exemple(&piece(3, &long)).unwrap();
        assert!(j.chars().count() <= EXEMPLE_CARACTERES_MAX);
        assert!(!j.contains(&"x".repeat(61)));
        let mut p = piece(0, "");
        p.entete
            .insert("numero_commande".into(), json!("y".repeat(5000)));
        // Un texte de l'en-tête est coupé aussi : l'exemple tient.
        assert!(json_exemple(&p).is_some());
        let b = bloc_exemple(&piece(1, "Bougie"), false).unwrap();
        assert!(b.contains("une autre marque du même groupe"));
        assert!(b.contains("ne recopie rien de cet exemple"));
    }

    #[test]
    fn corrections_resumees_par_champ() {
        let c = vec![
            json!({ "date_echeance": { "avant": "2026-11-30", "apres": "2026-12-31" },
                    "fournisseur_id": { "avant": 1, "apres": 2 } }),
            json!({}),
            json!({ "date_echeance": { "avant": null, "apres": "2026-10-31" },
                    "montant_ht": { "avant": 10.0, "apres": 12.5 } }),
            json!({ "date_echeance": { "avant": "x", "apres": "y" } }),
        ];
        let s = bloc_corrections(&c);
        assert!(
            s.contains("- echeance : corrigé 3 fois sur 4 (dernière fois : lu « 2026-11-30 », corrigé en « 2026-12-31 »)"),
            "{s}"
        );
        assert!(s.contains(
            "- montant_ht : corrigé 1 fois sur 4 (dernière fois : lu 10.0, corrigé en 12.5)"
        ));
        assert!(!s.contains("fournisseur_id"));
        assert!(s.find("echeance").unwrap() < s.find("montant_ht").unwrap());
        assert_eq!(bloc_corrections(&[json!({}), json!({})]), "");
    }

    #[test]
    fn type_devine_avant_lecture() {
        assert_eq!(
            type_probable(
                Some("SARL Durand\nFACTURE N° 12\nBon de livraison BL 4"),
                "",
                ""
            ),
            Some("facture")
        );
        assert_eq!(
            type_probable(Some("BON DE LIVRAISON 778\nAdresse de facturation"), "", ""),
            Some("bl")
        );
        assert_eq!(
            type_probable(Some("Avoir n° 55 sur facture 12"), "", ""),
            Some("avoir")
        );
        // « pour avoir » n'est pas un avoir.
        assert_eq!(
            type_probable(Some("Merci pour avoir choisi Durand. Facture 3"), "", ""),
            Some("facture")
        );
        assert_eq!(type_probable(None, "BL_2026-778.pdf", ""), Some("bl"));
        assert_eq!(
            type_probable(None, "scan001.pdf", "Votre facture"),
            Some("facture")
        );
        assert_eq!(type_probable(None, "scan001.pdf", "Bonjour"), None);
    }
}
