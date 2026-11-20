//! Factures électroniques : XML CII (Factur-X, ZUGFeRD) et UBL, lus sans appel à Claude.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Partie {
    pub nom: Option<String>,
    pub siret: Option<String>,
    pub tva: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DonneesXml {
    /// `facture` ou `avoir`.
    pub type_doc: String,
    pub numero: Option<String>,
    /// AAAA-MM-JJ.
    pub date: Option<String>,
    pub numero_commande: Option<String>,
    pub vendeur: Partie,
    pub acheteur: Partie,
    pub montant_ht: Option<f64>,
    pub montant_tva: Option<f64>,
    pub montant_ttc: Option<f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Cii,
    Ubl,
}

fn nom_local(s: &str) -> String {
    match s.rsplit_once(':') {
        Some((_, l)) => l.to_string(),
        None => s.to_string(),
    }
}

fn montant(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

/// AAAAMMJJ (CII, format 102) ou AAAA-MM-JJ (UBL) -> AAAA-MM-JJ.
fn date_iso(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        return Some(format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8]));
    }
    if s.len() >= 10 && s.as_bytes()[4] == b'-' {
        return Some(s[..10].to_string());
    }
    None
}

fn type_depuis_code(code: &str) -> &'static str {
    match code.trim() {
        "381" | "261" | "262" | "396" | "532" => "avoir",
        _ => "facture",
    }
}

fn remplir(champ: &mut Option<String>, v: &str) {
    if champ.is_none() && !v.trim().is_empty() {
        *champ = Some(v.trim().to_string());
    }
}

fn remplir_montant(champ: &mut Option<f64>, v: &str) {
    if champ.is_none() {
        *champ = montant(v);
    }
}

/// Lit une facture CII ou UBL. `None` si le XML n'est pas une facture reconnue.
pub fn lire(xml: &[u8]) -> Option<DonneesXml> {
    let mut lecteur = Reader::from_reader(xml);
    let mut pile: Vec<String> = Vec::new();
    // Attribut schemeID de l'élément courant (identifiants CII).
    let mut schema_id: Vec<Option<String>> = Vec::new();
    let mut format: Option<Format> = None;
    let mut d = DonneesXml::default();
    let mut code_type: Option<String> = None;
    let mut texte = String::new();
    let mut tampon = Vec::new();
    loop {
        match lecteur.read_event_into(&mut tampon) {
            Ok(Event::Start(e)) => {
                let nom = nom_local(e.name().as_ref());
                if pile.is_empty() {
                    format = match nom.as_str() {
                        "CrossIndustryInvoice" => Some(Format::Cii),
                        "Invoice" | "CreditNote" => Some(Format::Ubl),
                        _ => return None,
                    };
                    if nom == "CreditNote" {
                        code_type = Some("381".into());
                    }
                }
                let sid = e
                    .attributes()
                    .flatten()
                    .find(|a| nom_local(a.key.as_ref()) == "schemeID")
                    .map(|a| a.value.to_string());
                pile.push(nom);
                schema_id.push(sid);
                texte.clear();
            }
            Ok(Event::End(_)) => {
                let v = texte.trim();
                if !v.is_empty() {
                    let chemin = pile.join("/");
                    let sid = schema_id.last().cloned().flatten();
                    match format {
                        Some(Format::Cii) => {
                            cii(&chemin, sid.as_deref(), v, &mut d, &mut code_type)
                        }
                        Some(Format::Ubl) => ubl(&chemin, v, &mut d, &mut code_type),
                        None => {}
                    }
                }
                texte.clear();
                pile.pop();
                schema_id.pop();
            }
            Ok(Event::Text(t)) => texte.push_str(&t.xml10_content()),
            Ok(Event::CData(c)) => texte.push_str(&c.into_inner()),
            Ok(Event::GeneralRef(r)) => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    texte.push(c);
                } else {
                    texte.push_str(match r.xml10_content().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
        tampon.clear();
    }
    format?;
    d.type_doc = type_depuis_code(code_type.as_deref().unwrap_or("380")).into();
    Some(d)
}

fn partie_cii(suite: &str, sid: Option<&str>, v: &str, p: &mut Partie) {
    match suite {
        "Name" => remplir(&mut p.nom, v),
        "SpecifiedLegalOrganization/ID" if matches!(sid, None | Some("0002") | Some("0009")) => {
            remplir(&mut p.siret, v)
        }
        "SpecifiedTaxRegistration/ID" if matches!(sid, None | Some("VA")) => remplir(&mut p.tva, v),
        _ => {}
    }
}

fn cii(
    chemin: &str,
    sid: Option<&str>,
    v: &str,
    d: &mut DonneesXml,
    code_type: &mut Option<String>,
) {
    let c = chemin
        .strip_prefix("CrossIndustryInvoice/")
        .unwrap_or(chemin);
    const ACCORD: &str = "SupplyChainTradeTransaction/ApplicableHeaderTradeAgreement/";
    const TOTAUX: &str = "SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement/SpecifiedTradeSettlementHeaderMonetarySummation/";
    match c {
        "ExchangedDocument/ID" => remplir(&mut d.numero, v),
        "ExchangedDocument/TypeCode" => remplir(code_type, v),
        "ExchangedDocument/IssueDateTime/DateTimeString" => {
            if d.date.is_none() {
                d.date = date_iso(v)
            }
        }
        _ => {
            if let Some(s) = c.strip_prefix(ACCORD) {
                if let Some(s) = s.strip_prefix("SellerTradeParty/") {
                    partie_cii(s, sid, v, &mut d.vendeur);
                } else if let Some(s) = s.strip_prefix("BuyerTradeParty/") {
                    partie_cii(s, sid, v, &mut d.acheteur);
                } else if s == "BuyerOrderReferencedDocument/IssuerAssignedID" {
                    remplir(&mut d.numero_commande, v);
                }
            } else if let Some(s) = c.strip_prefix(TOTAUX) {
                match s {
                    "TaxBasisTotalAmount" => remplir_montant(&mut d.montant_ht, v),
                    "TaxTotalAmount" => remplir_montant(&mut d.montant_tva, v),
                    "GrandTotalAmount" => remplir_montant(&mut d.montant_ttc, v),
                    _ => {}
                }
            }
        }
    }
}

fn partie_ubl(suite: &str, v: &str, p: &mut Partie) {
    match suite {
        "PartyLegalEntity/RegistrationName" | "PartyName/Name" => remplir(&mut p.nom, v),
        "PartyLegalEntity/CompanyID" | "PartyIdentification/ID" => remplir(&mut p.siret, v),
        "PartyTaxScheme/CompanyID" => remplir(&mut p.tva, v),
        _ => {}
    }
}

fn ubl(chemin: &str, v: &str, d: &mut DonneesXml, code_type: &mut Option<String>) {
    let Some((_, c)) = chemin.split_once('/') else {
        return;
    };
    match c {
        "ID" => remplir(&mut d.numero, v),
        "IssueDate" => {
            if d.date.is_none() {
                d.date = date_iso(v)
            }
        }
        "InvoiceTypeCode" | "CreditNoteTypeCode" => remplir(code_type, v),
        "OrderReference/ID" => remplir(&mut d.numero_commande, v),
        "TaxTotal/TaxAmount" => remplir_montant(&mut d.montant_tva, v),
        "LegalMonetaryTotal/TaxExclusiveAmount" => remplir_montant(&mut d.montant_ht, v),
        "LegalMonetaryTotal/TaxInclusiveAmount" => remplir_montant(&mut d.montant_ttc, v),
        _ => {
            if let Some(s) = c.strip_prefix("AccountingSupplierParty/Party/") {
                partie_ubl(s, v, &mut d.vendeur);
            } else if let Some(s) = c.strip_prefix("AccountingCustomerParty/Party/") {
                partie_ubl(s, v, &mut d.acheteur);
            }
        }
    }
}
