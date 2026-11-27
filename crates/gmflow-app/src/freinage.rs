//! Freinage des essais de connexion (constat C10). Aucun compte n'est verrouillé : un tiers qui
//! tape de faux mots de passe ne ralentit que lui-même. Les échecs comptent par adresse IP et
//! compte (5 essais libres, puis une attente qui double à chaque échec : 30 s, 1 min, 2 min…
//! jusqu'à 15 min), et par adresse IP tous comptes confondus (20 essais libres), contre celui
//! qui essaie beaucoup d'identifiants. Le vrai utilisateur, depuis une autre adresse, se
//! connecte normalement.
//!
//! Adresse IP : l'app est derrière Nginx Proxy Manager, qui la donne dans `X-Real-IP` /
//! `X-Forwarded-For`. Ces en-têtes ne sont crus que si la connexion vient d'un proxy de
//! confiance (par défaut : adresses privées et locales, c'est-à-dire le réseau Docker ;
//! réglable par `GMFLOW_PROXY_CONFIANCE`). Sinon, c'est l'adresse de la connexion qui compte.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;

/// Échecs sans attente pour une même adresse et un même compte.
pub const ESSAIS_LIBRES_COMPTE: u32 = 5;
/// Échecs sans attente pour une même adresse, tous comptes confondus.
pub const ESSAIS_LIBRES_ADRESSE: u32 = 20;
/// Première attente imposée, doublée à chaque nouvel échec.
pub const ATTENTE_PREMIERE: Duration = Duration::from_secs(30);
/// Attente la plus longue.
pub const ATTENTE_MAX: Duration = Duration::from_secs(15 * 60);
/// Les échecs sont oubliés après ce temps sans nouvel essai (une fois l'attente passée).
pub const OUBLI: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy)]
struct Suivi {
    echecs: u32,
    dernier: Instant,
    attente: Duration,
}

impl Suivi {
    fn libre_le(&self) -> Instant {
        self.dernier + self.attente
    }
}

/// Compteurs d'échecs en mémoire (une seule instance de l'app).
#[derive(Default)]
pub struct Freinage {
    suivis: Mutex<HashMap<String, Suivi>>,
}

fn attente_pour(echecs: u32, libres: u32) -> Duration {
    if echecs < libres {
        return Duration::ZERO;
    }
    let puissance = (echecs - libres).min(10);
    (ATTENTE_PREMIERE * 2u32.pow(puissance)).min(ATTENTE_MAX)
}

impl Freinage {
    /// Temps à attendre avant un nouvel essai pour ces clés (le plus long), `None` si libre.
    pub fn attente(&self, cles: &[&str], maintenant: Instant) -> Option<Duration> {
        let mut suivis = self.suivis.lock().unwrap();
        let mut plus_long = None;
        for cle in cles {
            let Some(s) = suivis.get(*cle).copied() else {
                continue;
            };
            if maintenant >= s.libre_le() + OUBLI {
                suivis.remove(*cle);
                continue;
            }
            if maintenant < s.libre_le() {
                let reste = s.libre_le() - maintenant;
                plus_long = Some(plus_long.map_or(reste, |p: Duration| p.max(reste)));
            }
        }
        plus_long
    }

    /// Réserve un essai pour ces clés (`(clé, échecs tolérés)`), d'un seul tenant : si l'une
    /// d'elles doit attendre, rend l'attente la plus longue ; sinon l'essai compte d'avance
    /// comme un échec sur chaque clé. Des essais lancés en parallèle ne passent donc pas tous
    /// le contrôle avant que le premier échec soit noté. Une réussite rend l'essai
    /// ([`Freinage::reussite`], [`Freinage::rendre`]).
    pub fn essayer(&self, cles: &[(&str, u32)], maintenant: Instant) -> Result<(), Duration> {
        let mut suivis = self.suivis.lock().unwrap();
        let mut plus_long: Option<Duration> = None;
        for (cle, _) in cles {
            let Some(s) = suivis.get(*cle).copied() else {
                continue;
            };
            if maintenant >= s.libre_le() + OUBLI {
                suivis.remove(*cle);
                continue;
            }
            if maintenant < s.libre_le() {
                let reste = s.libre_le() - maintenant;
                plus_long = Some(plus_long.map_or(reste, |p| p.max(reste)));
            }
        }
        if let Some(d) = plus_long {
            return Err(d);
        }
        for (cle, libres) in cles {
            noter_echec(&mut suivis, cle, *libres, maintenant);
        }
        Ok(())
    }

    /// Rend un essai réservé par [`Freinage::essayer`] qui a réussi, sans effacer les autres
    /// échecs de la clé (clé de l'adresse : une connexion réussie ne remet pas à zéro les
    /// essais faits sur d'autres comptes).
    pub fn rendre(&self, cle: &str, libres: u32) {
        let mut suivis = self.suivis.lock().unwrap();
        if let Some(s) = suivis.get_mut(cle) {
            s.echecs = s.echecs.saturating_sub(1);
            s.attente = attente_pour(s.echecs, libres);
            if s.echecs == 0 {
                suivis.remove(cle);
            }
        }
    }

    /// Note un échec pour la clé ; `libres` : échecs tolérés avant la première attente.
    pub fn echec(&self, cle: &str, libres: u32, maintenant: Instant) {
        noter_echec(&mut self.suivis.lock().unwrap(), cle, libres, maintenant);
    }

    /// Connexion réussie : les échecs de cette clé sont oubliés.
    pub fn reussite(&self, cle: &str) {
        self.suivis.lock().unwrap().remove(cle);
    }
}

fn noter_echec(suivis: &mut HashMap<String, Suivi>, cle: &str, libres: u32, maintenant: Instant) {
    {
        let s = suivis.entry(cle.to_string()).or_insert(Suivi {
            echecs: 0,
            dernier: maintenant,
            attente: Duration::ZERO,
        });
        if maintenant >= s.libre_le() + OUBLI {
            s.echecs = 0;
        }
        s.echecs += 1;
        s.dernier = maintenant;
        s.attente = attente_pour(s.echecs, libres);
    }
    // Évite que la table grossisse sans fin (essais depuis beaucoup d'adresses).
    if suivis.len() > 10_000 {
        suivis.retain(|_, s| maintenant < s.libre_le() + OUBLI);
    }
}

/// Réseaux dont la connexion peut annoncer l'adresse du client (proxy de confiance).
#[derive(Debug, Clone)]
pub struct ProxysDeConfiance(Vec<(IpAddr, u8)>);

fn reseau(s: &str) -> Option<(IpAddr, u8)> {
    let (ip, longueur) = match s.split_once('/') {
        Some((ip, l)) => (
            ip.trim().parse::<IpAddr>().ok()?,
            l.trim().parse::<u8>().ok()?,
        ),
        None => {
            let ip = s.trim().parse::<IpAddr>().ok()?;
            (ip, if ip.is_ipv4() { 32 } else { 128 })
        }
    };
    let max = if ip.is_ipv4() { 32 } else { 128 };
    (longueur <= max).then_some((ip, longueur))
}

fn dans(ip: IpAddr, (reseau, longueur): (IpAddr, u8)) -> bool {
    match (ip, reseau) {
        (IpAddr::V4(a), IpAddr::V4(r)) => {
            let masque = u32::MAX.checked_shl(32 - u32::from(longueur)).unwrap_or(0);
            u32::from(a) & masque == u32::from(r) & masque
        }
        (IpAddr::V6(a), IpAddr::V6(r)) => {
            let masque = u128::MAX
                .checked_shl(128 - u32::from(longueur))
                .unwrap_or(0);
            u128::from(a) & masque == u128::from(r) & masque
        }
        _ => false,
    }
}

impl ProxysDeConfiance {
    /// Adresses privées et locales : le réseau Docker où se trouve Nginx Proxy Manager.
    pub fn par_defaut() -> Self {
        Self::depuis_texte("127.0.0.0/8, ::1, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, fc00::/7")
    }

    /// Liste séparée par des virgules (`172.18.0.0/16, 10.0.0.5`) ; `aucun` : aucun proxy.
    pub fn depuis_texte(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("aucun") {
            return Self(Vec::new());
        }
        Self(s.split(',').filter_map(reseau).collect())
    }

    /// `GMFLOW_PROXY_CONFIANCE` si posé, sinon les adresses privées.
    pub fn depuis_env() -> Self {
        match std::env::var("GMFLOW_PROXY_CONFIANCE") {
            Ok(v) if !v.trim().is_empty() => Self::depuis_texte(&v),
            _ => Self::par_defaut(),
        }
    }

    pub fn contient(&self, ip: IpAddr) -> bool {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 => v4,
        };
        self.0.iter().any(|r| dans(ip, *r))
    }
}

/// Adresse du client : celle annoncée par un proxy de confiance (`X-Real-IP`, sinon la
/// dernière de `X-Forwarded-For`, celle que le proxy a ajoutée), sinon celle de la connexion.
pub fn adresse_client(
    pair: Option<IpAddr>,
    entetes: &HeaderMap,
    proxys: &ProxysDeConfiance,
) -> Option<IpAddr> {
    let pair = pair?;
    if !proxys.contient(pair) {
        return Some(pair);
    }
    let lue = |nom: &str| entetes.get(nom).and_then(|v| v.to_str().ok());
    if let Some(ip) = lue("x-real-ip").and_then(|v| v.trim().parse().ok()) {
        return Some(ip);
    }
    if let Some(ip) = lue("x-forwarded-for")
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse().ok())
    {
        return Some(ip);
    }
    Some(pair)
}

/// Clé de freinage d'une adresse : l'adresse IPv4 elle-même, le réseau /64 d'une adresse IPv6
/// (un abonné en reçoit un entier et peut en changer à chaque essai).
pub fn cle_reseau(adresse: &str) -> String {
    match adresse.parse::<IpAddr>() {
        Ok(IpAddr::V6(v6)) if v6.to_ipv4_mapped().is_none() => {
            let s = v6.segments();
            let reseau = std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0);
            format!("{reseau}/64")
        }
        Ok(IpAddr::V6(v6)) => v6
            .to_ipv4_mapped()
            .map_or(adresse.to_string(), |v4| v4.to_string()),
        _ => adresse.to_string(),
    }
}

/// Adresse du client de la requête (texte), « inconnue » sans information de connexion
/// (tests qui appellent le routeur directement).
pub struct AdresseClient(pub String);

impl FromRequestParts<crate::Etat> for AdresseClient {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        etat: &crate::Etat,
    ) -> Result<Self, Self::Rejection> {
        let pair = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Ok(AdresseClient(
            adresse_client(pair, &parts.headers, &etat.proxys)
                .map_or_else(|| "inconnue".to_string(), |ip| ip.to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attente_qui_double_sans_verrou() {
        let f = Freinage::default();
        let t0 = Instant::now();
        for _ in 0..4 {
            f.echec("a|michael", ESSAIS_LIBRES_COMPTE, t0);
        }
        assert_eq!(f.attente(&["a|michael"], t0), None);
        f.echec("a|michael", ESSAIS_LIBRES_COMPTE, t0);
        assert_eq!(f.attente(&["a|michael"], t0), Some(ATTENTE_PREMIERE));
        // Une autre adresse n'est pas freinée.
        assert_eq!(f.attente(&["b|michael"], t0), None);
        // Attente passée : un essai de plus, raté, double l'attente.
        let t1 = t0 + ATTENTE_PREMIERE;
        assert_eq!(f.attente(&["a|michael"], t1), None);
        f.echec("a|michael", ESSAIS_LIBRES_COMPTE, t1);
        assert_eq!(f.attente(&["a|michael"], t1), Some(ATTENTE_PREMIERE * 2));
        // Plafond.
        for _ in 0..20 {
            f.echec("a|michael", ESSAIS_LIBRES_COMPTE, t1);
        }
        assert_eq!(f.attente(&["a|michael"], t1), Some(ATTENTE_MAX));
        // Oubli après la fin de l'attente + 15 min.
        let t2 = t1 + ATTENTE_MAX + OUBLI;
        assert_eq!(f.attente(&["a|michael"], t2), None);
        f.echec("a|michael", ESSAIS_LIBRES_COMPTE, t2);
        assert_eq!(f.attente(&["a|michael"], t2), None);
        f.reussite("a|michael");
    }

    #[test]
    fn essais_reserves_d_avance() {
        let f = Freinage::default();
        let t0 = Instant::now();
        let cles = [
            ("a|michael", ESSAIS_LIBRES_COMPTE),
            ("a|*", ESSAIS_LIBRES_ADRESSE),
        ];
        // Essais lancés en même temps : chacun est compté avant sa vérification, le sixième
        // est refusé sans attendre que les cinq premiers aient échoué.
        for _ in 0..ESSAIS_LIBRES_COMPTE {
            assert_eq!(f.essayer(&cles, t0), Ok(()));
        }
        assert_eq!(f.essayer(&cles, t0), Err(ATTENTE_PREMIERE));
        // Une réussite rend son essai : la clé du compte est effacée, celle de l'adresse
        // perd un échec seulement.
        let g = Freinage::default();
        for _ in 0..3 {
            g.essayer(&cles, t0).unwrap();
        }
        g.reussite("a|michael");
        g.rendre("a|*", ESSAIS_LIBRES_ADRESSE);
        assert_eq!(
            g.suivis.lock().unwrap().get("a|*").map(|s| s.echecs),
            Some(2)
        );
        assert!(!g.suivis.lock().unwrap().contains_key("a|michael"));
    }

    #[test]
    fn en_tetes_crus_seulement_depuis_un_proxy() {
        let proxys = ProxysDeConfiance::par_defaut();
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", "203.0.113.7".parse().unwrap());
        h.insert("x-forwarded-for", "1.1.1.1, 198.51.100.2".parse().unwrap());
        let npm: IpAddr = "172.18.0.4".parse().unwrap();
        let public: IpAddr = "198.51.100.9".parse().unwrap();
        assert_eq!(
            adresse_client(Some(npm), &h, &proxys),
            Some("203.0.113.7".parse().unwrap())
        );
        assert_eq!(adresse_client(Some(public), &h, &proxys), Some(public));
        h.remove("x-real-ip");
        assert_eq!(
            adresse_client(Some(npm), &h, &proxys),
            Some("198.51.100.2".parse().unwrap())
        );
        assert_eq!(
            adresse_client(Some(npm), &h, &ProxysDeConfiance::depuis_texte("aucun")),
            Some(npm)
        );
        let un = ProxysDeConfiance::depuis_texte("10.0.0.5, 192.168.1.0/24");
        assert!(un.contient("10.0.0.5".parse().unwrap()));
        assert!(!un.contient("10.0.0.6".parse().unwrap()));
        assert!(un.contient("192.168.1.200".parse().unwrap()));
        assert!(un.contient("::ffff:192.168.1.3".parse().unwrap()));
        assert_eq!(adresse_client(None, &h, &proxys), None);
    }

    #[test]
    fn ipv6_freine_par_reseau() {
        assert_eq!(cle_reseau("2001:db8:1:2:aaaa::1"), "2001:db8:1:2::/64");
        assert_eq!(
            cle_reseau("2001:db8:1:2:bbbb:cccc::9"),
            cle_reseau("2001:db8:1:2::77")
        );
        assert_ne!(cle_reseau("2001:db8:1:3::1"), cle_reseau("2001:db8:1:2::1"));
        assert_eq!(cle_reseau("203.0.113.7"), "203.0.113.7");
        assert_eq!(cle_reseau("::ffff:203.0.113.7"), "203.0.113.7");
        assert_eq!(cle_reseau("inconnue"), "inconnue");
    }
}
