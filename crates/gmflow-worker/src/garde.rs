//! Garde de durée de l'analyse d'une pièce (constat C9) : un fichier piégé dont l'analyse ne
//! finit pas n'occupe pas la file de lecture. Au-delà de `limites::DUREE_MAX`, la pièce est
//! marquée trop volumineuse ; le fil bloquant finit seul en arrière-plan.

use gmflow_core::lecture::Aiguillage;
use gmflow_core::lecture::limites::{Depassement, limites};
use tokio::task::JoinHandle;

pub(crate) trait Borner {
    /// Attend l'analyse au plus `duree_max` (contenu vide si elle est abandonnée).
    async fn bornee(self) -> anyhow::Result<(Vec<u8>, Aiguillage)>;
}

type Analyse = JoinHandle<anyhow::Result<(Vec<u8>, Aiguillage)>>;

impl Borner for Analyse {
    async fn bornee(self) -> anyhow::Result<(Vec<u8>, Aiguillage)> {
        borner(self, limites().duree_max).await
    }
}

async fn borner(h: Analyse, duree: std::time::Duration) -> anyhow::Result<(Vec<u8>, Aiguillage)> {
    match tokio::time::timeout(duree, h).await {
        Ok(r) => r?,
        Err(_) => {
            tracing::warn!(secondes = duree.as_secs(), "analyse d'une pièce abandonnée");
            Ok((
                Vec::new(),
                Aiguillage::TropVolumineux(Depassement::Duree {
                    secondes: duree.as_secs(),
                }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn analyse_trop_longue_abandonnee() {
        let h = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok((vec![1], Aiguillage::Ignore("fini".into())))
        });
        let (contenu, a) = borner(h, std::time::Duration::from_millis(50))
            .await
            .unwrap();
        assert!(contenu.is_empty());
        assert!(matches!(
            a,
            Aiguillage::TropVolumineux(Depassement::Duree { .. })
        ));
        let h = tokio::spawn(async { Ok((vec![1], Aiguillage::Ignore("fini".into()))) });
        assert_eq!(h.bornee().await.unwrap().0, vec![1]);
    }
}
