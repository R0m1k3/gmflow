//! Fichiers `.eml` sur le volume : `eml/<2 premiers caractères>/<sha256>.eml`.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::eml::sha256;

/// Écrit le `.eml` (fichier temporaire, fsync puis renommage) et rend (chemin relatif, sha256).
/// Rien n'est réécrit si le fichier existe déjà.
pub fn ecrire_eml(racine: &Path, brut: &[u8]) -> std::io::Result<(String, Vec<u8>)> {
    let empreinte = sha256(brut);
    let hexa = hex::encode(&empreinte);
    let relatif = format!("eml/{}/{}.eml", &hexa[..2], hexa);
    let chemin = racine.join(&relatif);
    if chemin.exists() {
        return Ok((relatif, empreinte));
    }
    let dossier = chemin.parent().expect("chemin avec dossier");
    std::fs::create_dir_all(dossier)?;
    let tmp = dossier.join(format!("{hexa}.{}.tmp", uuid::Uuid::new_v4()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(brut)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &chemin)?;
    if let Ok(d) = std::fs::File::open(dossier) {
        let _ = d.sync_all();
    }
    Ok((relatif, empreinte))
}

pub fn chemin_absolu(racine: &Path, relatif: &str) -> PathBuf {
    racine.join(relatif)
}
