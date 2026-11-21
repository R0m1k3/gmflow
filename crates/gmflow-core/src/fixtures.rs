//! Fichiers de test fabriqués à la volée (fonction `test-utils`) : PDF texte, PDF scanné,
//! Factur-X, XML CII et UBL, classeur xlsx, et mail MIME avec pièces jointes.

use base64::Engine;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, Stream, StringFormat, dictionary};

pub const SIRET_A: &str = "12345678900014";
pub const SIRET_B: &str = "98765432100027";

fn page_texte(doc: &mut Document, pages_id: lopdf::ObjectId, lignes: &[&str]) -> lopdf::ObjectId {
    let police = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let ressources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => police } });
    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 11.into()]),
    ];
    let mut y = 800;
    for l in lignes {
        ops.push(Operation::new(
            "Tm",
            vec![1.into(), 0.into(), 0.into(), 1.into(), 50.into(), y.into()],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(l.bytes().collect(), StringFormat::Literal)],
        ));
        y -= 16;
    }
    ops.push(Operation::new("ET", vec![]));
    let contenu = doc.add_object(Stream::new(
        Dictionary::new(),
        Content { operations: ops }.encode().unwrap(),
    ));
    doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => contenu,
        "Resources" => ressources, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    })
}

fn finir(
    mut doc: Document,
    pages_id: lopdf::ObjectId,
    kids: Vec<lopdf::ObjectId>,
    catalogue_en_plus: Dictionary,
) -> Vec<u8> {
    let n = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids.into_iter().map(Object::from).collect::<Vec<_>>(), "Count" => n,
        }),
    );
    let mut cat = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    for (k, v) in catalogue_en_plus {
        cat.set(k, v);
    }
    let cat_id = doc.add_object(cat);
    doc.trailer.set("Root", cat_id);
    let mut sortie = Vec::new();
    doc.save_to(&mut sortie).unwrap();
    sortie
}

/// PDF d'une page avec une couche texte (une ligne par élément).
pub fn pdf_texte(lignes: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page = page_texte(&mut doc, pages_id, lignes);
    finir(doc, pages_id, vec![page], Dictionary::new())
}

/// Faux JPEG (en-tête JPEG, contenu aléatoire) de `taille` octets : assez pour l'aiguillage.
pub fn jpeg(taille: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    let mut x: u32 = 0x1234_5678;
    while v.len() < taille {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        v.push((x & 0xFF) as u8);
    }
    v
}

/// PNG minimal (1x1), comme un logo de signature.
pub fn png_logo() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend(std::iter::repeat_n(0u8, 2000));
    v
}

/// PDF d'une page faite d'une seule image, sans couche texte (scan).
pub fn pdf_scan() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 100, "Height" => 100,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        },
        jpeg(70_000),
    ));
    let ressources = doc.add_object(dictionary! { "XObject" => dictionary! { "Im1" => image } });
    let contenu = doc.add_object(Stream::new(
        Dictionary::new(),
        b"q 595 0 0 842 0 0 cm /Im1 Do Q".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => contenu,
        "Resources" => ressources, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    finir(doc, pages_id, vec![page], Dictionary::new())
}

/// Facture CII (profil Factur-X), acheteur identifié par `siret_acheteur`.
pub fn xml_cii(numero: &str, siret_acheteur: &str, avoir: bool) -> String {
    let code = if avoir { "381" } else { "380" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">
  <rsm:ExchangedDocumentContext><ram:GuidelineSpecifiedDocumentContextParameter><ram:ID>urn:factur-x.eu:1p0:basic</ram:ID></ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext>
  <rsm:ExchangedDocument>
    <ram:ID>{numero}</ram:ID>
    <ram:TypeCode>{code}</ram:TypeCode>
    <ram:IssueDateTime><udt:DateTimeString format="102">20261112</udt:DateTimeString></ram:IssueDateTime>
  </rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
    <ram:ApplicableHeaderTradeAgreement>
      <ram:SellerTradeParty>
        <ram:Name>Grossiste Durand &amp; Fils</ram:Name>
        <ram:SpecifiedLegalOrganization><ram:ID schemeID="0002">55210055400013</ram:ID></ram:SpecifiedLegalOrganization>
        <ram:SpecifiedTaxRegistration><ram:ID schemeID="VA">FR40552100554</ram:ID></ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
      <ram:BuyerTradeParty>
        <ram:Name>Magasin</ram:Name>
        <ram:SpecifiedLegalOrganization><ram:ID schemeID="0002">{siret_acheteur}</ram:ID></ram:SpecifiedLegalOrganization>
      </ram:BuyerTradeParty>
      <ram:BuyerOrderReferencedDocument><ram:IssuerAssignedID>CMD-77</ram:IssuerAssignedID></ram:BuyerOrderReferencedDocument>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>
        <ram:TaxBasisTotalAmount>100.00</ram:TaxBasisTotalAmount>
        <ram:TaxTotalAmount currencyID="EUR">20.00</ram:TaxTotalAmount>
        <ram:GrandTotalAmount>120.00</ram:GrandTotalAmount>
      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>
    </ram:ApplicableHeaderTradeSettlement>
  </rsm:SupplyChainTradeTransaction>
</rsm:CrossIndustryInvoice>"#
    )
}

/// Avoir UBL.
pub fn xml_ubl_avoir(numero: &str, tva_acheteur: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CreditNote xmlns="urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>{numero}</cbc:ID>
  <cbc:IssueDate>2026-11-03</cbc:IssueDate>
  <cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyName><cbc:Name>Fournitures Martin</cbc:Name></cac:PartyName>
    <cac:PartyTaxScheme><cbc:CompanyID>FR11222333444</cbc:CompanyID></cac:PartyTaxScheme>
  </cac:Party></cac:AccountingSupplierParty>
  <cac:AccountingCustomerParty><cac:Party>
    <cac:PartyTaxScheme><cbc:CompanyID>{tva_acheteur}</cbc:CompanyID></cac:PartyTaxScheme>
  </cac:Party></cac:AccountingCustomerParty>
  <cac:TaxTotal><cbc:TaxAmount currencyID="EUR">2.00</cbc:TaxAmount></cac:TaxTotal>
  <cac:LegalMonetaryTotal>
    <cbc:TaxExclusiveAmount currencyID="EUR">10.00</cbc:TaxExclusiveAmount>
    <cbc:TaxInclusiveAmount currencyID="EUR">12.00</cbc:TaxInclusiveAmount>
  </cac:LegalMonetaryTotal>
</CreditNote>"#
    )
}

/// PDF Factur-X : page texte et XML CII embarqué (EmbeddedFiles et AF).
pub fn pdf_facturx(xml: &str, lignes: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page = page_texte(&mut doc, pages_id, lignes);
    let mut flux = Stream::new(
        dictionary! { "Type" => "EmbeddedFile", "Subtype" => "text/xml" },
        xml.as_bytes().to_vec(),
    );
    flux.compress().unwrap();
    let flux = doc.add_object(flux);
    let fs = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => Object::string_literal("factur-x.xml"),
        "UF" => Object::string_literal("factur-x.xml"),
        "AFRelationship" => "Data",
        "EF" => dictionary! { "F" => flux, "UF" => flux },
    });
    let ef = doc.add_object(dictionary! {
        "Names" => vec![Object::string_literal("factur-x.xml"), fs.into()],
    });
    let en_plus = dictionary! {
        "Names" => dictionary! { "EmbeddedFiles" => ef },
        "AF" => vec![Object::from(fs)],
    };
    finir(doc, pages_id, vec![page], en_plus)
}

/// Classeur xlsx d'une feuille (lignes de cellules texte).
pub fn xlsx(lignes: &[&[&str]]) -> Vec<u8> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("BL").unwrap();
    for (i, l) in lignes.iter().enumerate() {
        for (j, c) in l.iter().enumerate() {
            match c.parse::<f64>() {
                Ok(n) => ws.write_number(i as u32, j as u16, n).unwrap(),
                Err(_) => ws.write_string(i as u32, j as u16, *c).unwrap(),
            };
        }
    }
    wb.save_to_buffer().unwrap()
}

/// Mail MIME (texte + pièces jointes en base64). `liste` : ajoute `List-Unsubscribe`.
pub fn eml(message_id: &str, objet: &str, pieces: &[(&str, &str, &[u8])], liste: bool) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut m = format!(
        "From: Fournisseur <factures@fournisseur.test>\r\nTo: magasin@test.local\r\n\
         Subject: {objet}\r\nMessage-ID: <{message_id}>\r\nDate: Thu, 12 Nov 2026 10:00:00 +0100\r\n\
         MIME-Version: 1.0\r\n"
    );
    if liste {
        m.push_str("List-Unsubscribe: <https://fournisseur.test/desabonnement>\r\n");
    }
    m.push_str("Content-Type: multipart/mixed; boundary=\"LIMITE\"\r\n\r\n");
    m.push_str("--LIMITE\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nVeuillez trouver ci-joint notre document.\r\n");
    for (nom, mime, contenu) in pieces {
        m.push_str(&format!(
            "--LIMITE\r\nContent-Type: {mime}; name=\"{nom}\"\r\nContent-Disposition: attachment; filename=\"{nom}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n"
        ));
        let enc = b64.encode(contenu);
        for morceau in enc.as_bytes().chunks(76) {
            m.push_str(std::str::from_utf8(morceau).unwrap());
            m.push_str("\r\n");
        }
    }
    m.push_str("--LIMITE--\r\n");
    m.into_bytes()
}
