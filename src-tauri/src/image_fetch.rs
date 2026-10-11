// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Récupération d'une image distante À LA DEMANDE de l'utilisateur
// (remplace l'ancien proxy d'images, constat D-05).
//
// L'image n'est jamais chargée automatiquement : le frontend n'appelle ce
// module qu'après un clic explicite. Garde-fous contre la SSRF :
//   - schéma http/https uniquement, ports 80 et 443, pas d'identifiants dans l'URL ;
//   - TOUTES les adresses résolues doivent être publiques (pas de boucle locale,
//     réseaux privés, link-local / métadonnées cloud, CGNAT, multicast…) ;
//   - l'adresse vérifiée est ÉPINGLÉE dans le client (pas de nouvelle résolution
//     DNS entre la vérification et la requête : anti DNS rebinding) ;
//   - redirections suivies à la main (3 max), chacune entièrement re-validée ;
//   - types raster uniquement (png, jpeg, gif, webp — pas de SVG) ;
//   - taille plafonnée (8 Mo) y compris sans Content-Length.
// ============================================================================

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::{header, redirect::Policy, Client, Url};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(15);
const ALLOWED_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/// Vrai si l'adresse est publique (routable sur Internet).
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // CGNAT 100.64.0.0/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // bancs d'essai 198.18.0.0/15
        || o[0] >= 240) // réservé
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let s = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (s[0] & 0xFE00) == 0xFC00 // unique local fc00::/7
        || (s[0] & 0xFFC0) == 0xFE80 // link-local fe80::/10
        || (s[0] == 0x2001 && s[1] == 0x0DB8) // documentation
        || (s[0] == 0x0064 && s[1] == 0xFF9B)) // NAT64 (peut cibler un IPv4 interne)
}

/// Valide l'URL et construit un client dont la résolution DNS est épinglée.
async fn client_for(url: &Url) -> Result<Client, String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Seules les URL http(s) sont autorisées".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL avec identifiants refusée".into());
    }
    let port = url.port_or_known_default().unwrap_or(0);
    if port != 80 && port != 443 {
        return Err("Port non autorisé (80 et 443 uniquement)".into());
    }

    let mut builder = Client::builder()
        .redirect(Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Demeter");

    let host = url
        .host_str()
        .ok_or_else(|| "URL sans hôte".to_string())?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();

    if let Ok(literal) = host.parse::<IpAddr>() {
        // Hôte donné sous forme d'adresse IP littérale
        if !is_public_ip(literal) {
            return Err("Adresse non publique refusée".into());
        }
    } else {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| "Résolution DNS impossible".to_string())?
            .collect();
        if addrs.is_empty() || !addrs.iter().all(|a| is_public_ip(a.ip())) {
            return Err("Adresse non publique refusée".into());
        }
        // Épinglage : la requête utilisera exactement l'adresse vérifiée.
        builder = builder.resolve(&host, addrs[0]);
    }
    builder.build().map_err(|e| e.to_string())
}

/// Récupère une image distante. Retourne (octets, type MIME validé).
pub async fn fetch_image(raw_url: &str) -> Result<(Vec<u8>, &'static str), String> {
    let mut url = Url::parse(raw_url.trim()).map_err(|_| "URL invalide".to_string())?;

    for _ in 0..=MAX_REDIRECTS {
        let client = client_for(&url).await?;
        let mut resp = client
            .get(url.clone())
            .header(header::ACCEPT, ALLOWED_TYPES.join(","))
            .send()
            .await
            .map_err(|_| "Serveur injoignable".to_string())?;

        if resp.status().is_redirection() {
            let loc = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "Redirection invalide".to_string())?;
            url = url
                .join(loc)
                .map_err(|_| "Redirection invalide".to_string())?;
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("Le serveur a répondu {}", resp.status().as_u16()));
        }

        let ctype = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| {
                v.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
            .unwrap_or_default();
        let mime = ALLOWED_TYPES
            .iter()
            .copied()
            .find(|t| *t == ctype)
            .ok_or_else(|| "Type de contenu non autorisé (images raster uniquement)".to_string())?;

        if resp
            .content_length()
            .map(|l| l as usize > MAX_BYTES)
            .unwrap_or(false)
        {
            return Err("Image trop volumineuse (8 Mo maximum)".into());
        }
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|_| "Lecture interrompue".to_string())?
        {
            if body.len() + chunk.len() > MAX_BYTES {
                return Err("Image trop volumineuse (8 Mo maximum)".into());
            }
            body.extend_from_slice(&chunk);
        }
        return Ok((body, mime));
    }
    Err("Trop de redirections".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn refuse_adresses_non_publiques() {
        for a in [
            "127.0.0.1",
            "127.1.2.3",
            "10.0.0.1",
            "172.16.5.4",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "100.127.255.254",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "192.0.2.1",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(ip(a)), "{a} devrait être refusée");
        }
    }

    #[test]
    fn accepte_adresses_publiques() {
        for a in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip(a)), "{a} devrait être acceptée");
        }
    }

    #[tokio::test]
    async fn refuse_urls_dangereuses_sans_reseau() {
        for u in [
            "http://127.0.0.1/x.png",
            "http://localhost/x.png",
            "http://[::1]/x.png",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.5:80/x.png",
            "https://user:pass@example.com/x.png",
            "ftp://example.com/x.png",
            "file:///etc/passwd",
            "http://example.com:8080/x.png",
            "pas une url",
        ] {
            assert!(fetch_image(u).await.is_err(), "{u} devrait être refusée");
        }
    }
}
