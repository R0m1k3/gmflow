//! Factures électroniques : XML CII (Factur-X, ZUGFeRD) et UBL, lus sans appel à Claude.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Serialize;

use crate::montant::Decimal;

use super::extraction::{LIGNES_MAX, Ligne, lignes_utiles, numeros_utiles};

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
    /// Échéance du paiement, AAAA-MM-JJ.
    pub echeance: Option<String>,
    pub vendeur: Partie,
    pub acheteur: Partie,
    pub montant_ht: Option<Decimal>,
    pub montant_tva: Option<Decimal>,
    pub montant_ttc: Option<Decimal>,
    /// Total des remises de pied (`AllowanceTotalAmount`), en positif.
    pub remise_ht: Option<Decimal>,
    /// Total des frais de pied (`ChargeTotalAmount` : port, éco-participation…).
    pub annexes_ht: Option<Decimal>,
    /// Lignes articles (au plus `LIGNES_MAX`).
    pub lignes: Vec<Ligne>,
    /// Numéros des bons de livraison cités (en-tête ou lignes).
    pub numeros_bl: Vec<String>,
    /// Devise des montants (`EUR`…), quand le XML la donne.
    pub devise: Option<String>,
    /// IBAN et BIC du compte du vendeur (moyen de paiement), tels qu'écrits.
    pub iban: Option<String>,
    pub bic: Option<String>,
    /// Plus de `LIGNES_MAX` lignes : les suivantes ne sont pas gardées.
    pub lignes_tronquees: bool,
    /// Quantité de base du prix de chaque ligne (prix pour 100 unités…), dans l'ordre des lignes.
    #[serde(skip)]
    bases: Vec<Option<Decimal>>,
    /// UBL : identifiant `PartyIdentification/ID` du vendeur et de l'acheteur, gardé comme
    /// SIRET seulement s'il en est un et que `PartyLegalEntity/CompanyID` manque.
    #[serde(skip)]
    secours: (Option<String>, Option<String>),
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

fn montant(s: &str) -> Option<Decimal> {
    crate::montant::depuis_texte(&s.trim().replace(',', "."))
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

fn remplir_montant(champ: &mut Option<Decimal>, v: &str) {
    if champ.is_none() {
        *champ = montant(v);
    }
}

/// Lit une facture CII ou UBL. `None` si le XML n'est pas une facture reconnue.
pub fn lire(xml: &[u8]) -> Option<DonneesXml> {
    let mut lecteur = Reader::from_reader(xml);
    let mut pile: Vec<String> = Vec::new();
    // Attributs schemeID (identifiants CII) et unitCode (quantités) de chaque élément ouvert.
    let mut attributs: Vec<(Option<String>, Option<String>)> = Vec::new();
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
                let attribut = |cle: &str| {
                    e.attributes()
                        .flatten()
                        .find(|a| nom_local(a.key.as_ref()) == cle)
                        .map(|a| a.value.to_string())
                };
                let attrs = (attribut("schemeID"), attribut("unitCode"));
                // Début d'une ligne article.
                let ligne = match (format, pile.len(), nom.as_str()) {
                    (Some(Format::Cii), 2, "IncludedSupplyChainTradeLineItem") => pile
                        .get(1)
                        .is_some_and(|p| p == "SupplyChainTradeTransaction"),
                    (Some(Format::Ubl), 1, "InvoiceLine" | "CreditNoteLine") => true,
                    _ => false,
                };
                if ligne && d.lignes.len() <= LIGNES_MAX {
                    d.lignes.push(Ligne::default());
                    d.bases.push(None);
                }
                pile.push(nom);
                attributs.push(attrs);
                texte.clear();
            }
            Ok(Event::End(_)) => {
                let v = texte.trim();
                if !v.is_empty() {
                    let chemin = pile.join("/");
                    let (sid, unite) = attributs.last().cloned().unwrap_or_default();
                    match format {
                        Some(Format::Cii) => {
                            cii(&chemin, sid.as_deref(), unite, v, &mut d, &mut code_type)
                        }
                        Some(Format::Ubl) => ubl(&chemin, unite, v, &mut d, &mut code_type),
                        None => {}
                    }
                }
                texte.clear();
                pile.pop();
                attributs.pop();
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
    // Prix pour 100 unités (ou toute autre quantité de base) : ramené à l'unité.
    for (l, b) in d.lignes.iter_mut().zip(std::mem::take(&mut d.bases)) {
        if let (Some(p), Some(b)) = (l.prix_unitaire_ht, b)
            && b > Decimal::ZERO
            && b != Decimal::ONE
        {
            l.prix_unitaire_ht = p.checked_div(b);
        }
    }
    d.lignes.iter_mut().for_each(Ligne::normaliser);
    for m in [
        &mut d.montant_ht,
        &mut d.montant_tva,
        &mut d.montant_ttc,
        &mut d.remise_ht,
        &mut d.annexes_ht,
    ] {
        *m = m.and_then(crate::montant::montant);
    }
    let (sv, sa) = std::mem::take(&mut d.secours);
    for (p, secours) in [(&mut d.vendeur, sv), (&mut d.acheteur, sa)] {
        if p.siret.is_none() {
            p.siret = secours;
        }
    }
    d.lignes_tronquees = d.lignes.iter().filter(|l| !l.vide()).count() > LIGNES_MAX;
    d.lignes = lignes_utiles(std::mem::take(&mut d.lignes));
    d.numeros_bl = numeros_utiles(std::mem::take(&mut d.numeros_bl));
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

/// EAN / GTIN : 8, 12, 13 ou 14 chiffres.
pub fn est_gencod(v: &str) -> bool {
    let v = v.trim();
    matches!(v.len(), 8 | 12 | 13 | 14) && v.bytes().all(|b| b.is_ascii_digit())
}

/// EAN / GTIN au bon format et dont la clé de contrôle GS1 (modulo 10) est juste. Les espaces
/// sont ignorés (« 3 123450 000011 »).
pub fn gencod_valide(v: &str) -> bool {
    let v: String = v.split_whitespace().collect();
    if !est_gencod(&v) {
        return false;
    }
    let chiffres: Vec<u32> = v.bytes().map(|b| u32::from(b - b'0')).collect();
    let (corps, cle) = chiffres.split_at(chiffres.len() - 1);
    let somme: u32 = corps
        .iter()
        .rev()
        .enumerate()
        .map(|(i, c)| if i % 2 == 0 { c * 3 } else { *c })
        .sum();
    (10 - somme % 10) % 10 == cle[0]
}

fn quantite(ligne: &mut Ligne, v: &str, unite: Option<String>) {
    if ligne.quantite.is_none() {
        ligne.quantite = montant(v);
        ligne.unite = unite;
    }
}

fn ligne_cii(s: &str, unite: Option<String>, v: &str, d: &mut DonneesXml) {
    if let Some(n) = s.strip_suffix("DespatchAdviceReferencedDocument/IssuerAssignedID")
        && n.ends_with("SpecifiedLineTradeDelivery/")
    {
        d.numeros_bl.push(v.to_string());
        return;
    }
    if s == "SpecifiedLineTradeAgreement/NetPriceProductTradePrice/BasisQuantity" {
        if let Some(b) = d.bases.last_mut() {
            *b = montant(v);
        }
        return;
    }
    let Some(l) = d.lignes.last_mut() else {
        return;
    };
    match s {
        // La référence du vendeur l'emporte (le GTIN, lu avant elle, va dans le gencod).
        "SpecifiedTradeProduct/SellerAssignedID" => l.reference = Some(v.to_string()),
        "SpecifiedTradeProduct/GlobalID" => {
            if gencod_valide(v) {
                remplir(&mut l.gencod, v);
            } else {
                remplir(&mut l.reference, v)
            }
        }
        "SpecifiedTradeProduct/BuyerAssignedID" => remplir(&mut l.reference, v),
        "SpecifiedLineTradeSettlement/ApplicableTradeTax/RateApplicablePercent" => {
            remplir_montant(&mut l.taux_tva, v)
        }
        "SpecifiedTradeProduct/Name" => remplir(&mut l.designation, v),
        "SpecifiedLineTradeDelivery/BilledQuantity" => quantite(l, v, unite),
        "SpecifiedLineTradeAgreement/NetPriceProductTradePrice/ChargeAmount" => {
            remplir_montant(&mut l.prix_unitaire_ht, v)
        }
        "SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount" => {
            remplir_montant(&mut l.montant_ht, v)
        }
        _ => {}
    }
}

fn cii(
    chemin: &str,
    sid: Option<&str>,
    unite: Option<String>,
    v: &str,
    d: &mut DonneesXml,
    code_type: &mut Option<String>,
) {
    let c = chemin
        .strip_prefix("CrossIndustryInvoice/")
        .unwrap_or(chemin);
    if let Some(s) = c.strip_prefix("SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem/")
    {
        ligne_cii(s, unite, v, d);
        return;
    }
    if c == "SupplyChainTradeTransaction/ApplicableHeaderTradeDelivery/DespatchAdviceReferencedDocument/IssuerAssignedID"
    {
        d.numeros_bl.push(v.to_string());
        return;
    }
    const ACCORD: &str = "SupplyChainTradeTransaction/ApplicableHeaderTradeAgreement/";
    const TOTAUX: &str = "SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement/SpecifiedTradeSettlementHeaderMonetarySummation/";
    match c {
        "SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement/InvoiceCurrencyCode" => {
            remplir(&mut d.devise, v)
        }
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
            } else if c
                == "SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement/SpecifiedTradePaymentTerms/DueDateDateTime/DateTimeString"
            {
                if d.echeance.is_none() {
                    d.echeance = date_iso(v)
                }
            } else if let Some(s) = c.strip_prefix(
                "SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement/SpecifiedTradeSettlementPaymentMeans/",
            ) {
                match s {
                    "PayeePartyCreditorFinancialAccount/IBANID" => remplir(&mut d.iban, v),
                    "PayeeSpecifiedCreditorFinancialInstitution/BICID" => remplir(&mut d.bic, v),
                    _ => {}
                }
            } else if let Some(s) = c.strip_prefix(TOTAUX) {
                match s {
                    "TaxBasisTotalAmount" => remplir_montant(&mut d.montant_ht, v),
                    "TaxTotalAmount" => remplir_montant(&mut d.montant_tva, v),
                    "GrandTotalAmount" => remplir_montant(&mut d.montant_ttc, v),
                    "AllowanceTotalAmount" => remplir_montant(&mut d.remise_ht, v),
                    "ChargeTotalAmount" => remplir_montant(&mut d.annexes_ht, v),
                    _ => {}
                }
            }
        }
    }
}

fn partie_ubl(suite: &str, v: &str, p: &mut Partie, secours: &mut Option<String>) {
    match suite {
        "PartyLegalEntity/RegistrationName" | "PartyName/Name" => remplir(&mut p.nom, v),
        "PartyLegalEntity/CompanyID" => remplir(&mut p.siret, v),
        // Souvent un GLN (13 chiffres) : gardé seulement s'il est un SIRET ou un SIREN.
        "PartyIdentification/ID" => {
            if crate::controles::siret_valide(v) || crate::controles::siren_valide(v) {
                remplir(secours, v)
            }
        }
        "PartyTaxScheme/CompanyID" => remplir(&mut p.tva, v),
        _ => {}
    }
}

fn ligne_ubl(s: &str, unite: Option<String>, v: &str, d: &mut DonneesXml) {
    if s == "DespatchLineReference/DocumentReference/ID" {
        d.numeros_bl.push(v.to_string());
        return;
    }
    if s == "Price/BaseQuantity" {
        if let Some(b) = d.bases.last_mut() {
            *b = montant(v);
        }
        return;
    }
    let Some(l) = d.lignes.last_mut() else {
        return;
    };
    match s {
        "Item/SellersItemIdentification/ID" => l.reference = Some(v.to_string()),
        "Item/StandardItemIdentification/ID" => {
            if gencod_valide(v) {
                remplir(&mut l.gencod, v);
            } else {
                remplir(&mut l.reference, v)
            }
        }
        "Item/ClassifiedTaxCategory/Percent" => remplir_montant(&mut l.taux_tva, v),
        // Le nom court l'emporte sur la description (imprimée avant lui, souvent longue).
        "Item/Name" => l.designation = Some(v.to_string()),
        "Item/Description" => remplir(&mut l.designation, v),
        "InvoicedQuantity" | "CreditedQuantity" => quantite(l, v, unite),
        "Price/PriceAmount" => remplir_montant(&mut l.prix_unitaire_ht, v),
        "LineExtensionAmount" => remplir_montant(&mut l.montant_ht, v),
        _ => {}
    }
}

fn ubl(
    chemin: &str,
    unite: Option<String>,
    v: &str,
    d: &mut DonneesXml,
    code_type: &mut Option<String>,
) {
    let Some((_, c)) = chemin.split_once('/') else {
        return;
    };
    if let Some(s) = c
        .strip_prefix("InvoiceLine/")
        .or_else(|| c.strip_prefix("CreditNoteLine/"))
    {
        ligne_ubl(s, unite, v, d);
        return;
    }
    match c {
        "DespatchDocumentReference/ID" => d.numeros_bl.push(v.to_string()),
        "DocumentCurrencyCode" => remplir(&mut d.devise, v),
        "ID" => remplir(&mut d.numero, v),
        "IssueDate" => {
            if d.date.is_none() {
                d.date = date_iso(v)
            }
        }
        "InvoiceTypeCode" | "CreditNoteTypeCode" => remplir(code_type, v),
        "OrderReference/ID" => remplir(&mut d.numero_commande, v),
        "DueDate" | "PaymentMeans/PaymentDueDate" => {
            if d.echeance.is_none() {
                d.echeance = date_iso(v)
            }
        }
        "TaxTotal/TaxAmount" => remplir_montant(&mut d.montant_tva, v),
        "LegalMonetaryTotal/TaxExclusiveAmount" => remplir_montant(&mut d.montant_ht, v),
        "LegalMonetaryTotal/TaxInclusiveAmount" => remplir_montant(&mut d.montant_ttc, v),
        "LegalMonetaryTotal/AllowanceTotalAmount" => remplir_montant(&mut d.remise_ht, v),
        "LegalMonetaryTotal/ChargeTotalAmount" => remplir_montant(&mut d.annexes_ht, v),
        "PaymentMeans/PayeeFinancialAccount/ID" => remplir(&mut d.iban, v),
        "PaymentMeans/PayeeFinancialAccount/FinancialInstitutionBranch/ID" => {
            remplir(&mut d.bic, v)
        }
        _ => {
            if let Some(s) = c.strip_prefix("AccountingSupplierParty/Party/") {
                partie_ubl(s, v, &mut d.vendeur, &mut d.secours.0);
            } else if let Some(s) = c.strip_prefix("AccountingCustomerParty/Party/") {
                partie_ubl(s, v, &mut d.acheteur, &mut d.secours.1);
            }
        }
    }
}
