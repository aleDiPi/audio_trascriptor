//! Testi legali incorporati nel programma (stessi file della cartella `legal/`, mostrati anche
//! dall'installer): l'avviso sui servizi AI di terzi si legge anche dalla finestra Impostazioni.

#![allow(dead_code)] // le funzioni le collega alla finestra Impostazioni la traduzione dell'interfaccia

const DISCLAIMER_EN: &str = include_str!("../legal/DISCLAIMER.en.txt");
const DISCLAIMER_IT: &str = include_str!("../legal/DISCLAIMER.it.txt");
const LICENSE_EN: &str = include_str!("../LICENSE");
const LICENSE_IT: &str = include_str!("../legal/LICENSE.it.txt");

/// Avviso sui servizi AI di terzi nella lingua richiesta ("it" o altro = inglese).
pub fn disclaimer(lang: &str) -> &'static str {
    if lang.to_lowercase().starts_with("it") { DISCLAIMER_IT } else { DISCLAIMER_EN }
}

/// Testo della licenza nella lingua richiesta ("it" o altro = inglese, che prevale).
pub fn license(lang: &str) -> &'static str {
    if lang.to_lowercase().starts_with("it") { LICENSE_IT } else { LICENSE_EN }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, ALL};

    #[test]
    fn lingua_italiana_o_inglese() {
        assert_eq!(disclaimer("it"), DISCLAIMER_IT);
        assert_eq!(disclaimer("it-IT"), DISCLAIMER_IT);
        assert_eq!(disclaimer("en"), DISCLAIMER_EN);
        assert_eq!(disclaimer("fr"), DISCLAIMER_EN);
        assert_eq!(license("IT"), LICENSE_IT);
        assert_eq!(license(""), LICENSE_EN);
    }

    #[test]
    fn avviso_cita_ogni_servizio_in_rete() {
        for p in ALL.iter().filter(|p| **p != Provider::Local) {
            for t in [DISCLAIMER_EN, DISCLAIMER_IT] {
                assert!(t.contains(p.name()), "manca {} nell'avviso", p.name());
            }
        }
    }

    #[test]
    fn avviso_dice_che_non_si_garantisce_il_trattamento() {
        assert!(DISCLAIMER_EN.contains("does NOT control and does NOT guarantee"));
        assert!(DISCLAIMER_IT.contains("NON controlla e NON garantisce"));
        assert!(DISCLAIMER_EN.contains("not legal advice") && DISCLAIMER_IT.contains("non e' una consulenza legale"));
    }

    #[test]
    fn licenza_nelle_due_lingue_ha_gli_stessi_articoli() {
        let numeri = |t: &str| -> Vec<String> {
            t.lines()
                .filter_map(|l| l.split_once(". ").filter(|(n, _)| n.parse::<u8>().is_ok()))
                .map(|(n, _)| n.to_string())
                .collect()
        };
        assert_eq!(numeri(LICENSE_EN), numeri(LICENSE_IT));
        assert_eq!(numeri(LICENSE_EN).len(), 9);
    }

    #[test]
    fn testi_legali_senza_riferimenti_vietati_e_con_licenza_coerente() {
        let tutti = [
            DISCLAIMER_EN, DISCLAIMER_IT, LICENSE_EN, LICENSE_IT,
            include_str!("../legal/PRIVACY.en.md"), include_str!("../legal/PRIVACY.it.md"),
            include_str!("../THIRD_PARTY_NOTICES.md"), include_str!("../CONTRIBUTING.md"),
        ];
        for t in tutti {
            assert!(!t.to_lowercase().contains("openwhisp"));
        }
        // Cargo.toml punta al file LICENSE; i testi non devono dichiararsi MIT o open source
        let cargo = include_str!("../Cargo.toml");
        assert!(cargo.contains("license-file = \"LICENSE\"") && !cargo.contains("license = \"MIT\""));
        assert!(LICENSE_EN.contains("NOT an open-source license"));
    }

    #[test]
    fn informativa_elenca_le_connessioni_reali() {
        for t in [include_str!("../legal/PRIVACY.en.md"), include_str!("../legal/PRIVACY.it.md")] {
            for host in ["api.groq.com", "api.openai.com", "api.anthropic.com", "api.github.com", "huggingface.co"] {
                assert!(t.contains(host), "manca {host}");
            }
        }
        // gli host nel codice sono quelli dichiarati
        for p in ALL.iter().filter(|p| **p != Provider::Local) {
            let host = p.base_url().trim_start_matches("https://").split('/').next().unwrap().to_string();
            assert!(include_str!("../legal/PRIVACY.en.md").contains(&host), "{host}");
        }
    }

    #[test]
    fn installer_mostra_l_avviso_in_ogni_lingua() {
        let iss = include_str!("../installer/trascrittore.iss");
        assert!(iss.contains("LicenseFile: \"..\\legal\\DISCLAIMER.it.txt\""));
    }
}
