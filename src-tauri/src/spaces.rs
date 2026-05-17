// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Ce fichier fait partie du projet Demeter.
// Vous pouvez le redistribuer et/ou le modifier selon les termes de la
// licence AGPL-3.0 publiée par la Free Software Foundation.
// ============================================================================


use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::models::{PromptItem, SpaceItem};

// Raw YAML structure (may differ slightly from API model)
#[derive(Debug, Serialize, Deserialize)]
struct YamlSpaces {
    spaces: Vec<YamlSpace>,
}

#[derive(Debug, Serialize, Deserialize)]
struct YamlSpace {
    id: String,
    #[serde(default = "default_icon")]
    icon: String,
    label: String,
    #[serde(default)]
    dot: String,
    #[serde(default)]
    system: String,
    #[serde(default)]
    prompts: Vec<YamlPrompt>,
}

#[derive(Debug, Serialize, Deserialize)]
struct YamlPrompt {
    #[serde(default = "default_icon")]
    icon: String,
    label: String,
    prompt: String,
}

fn default_icon() -> String {
    "💬".to_string()
}

pub fn load_spaces(path: &Path) -> Result<Vec<SpaceItem>> {
    let content = std::fs::read_to_string(path)?;
    let raw: YamlSpaces = serde_yaml::from_str(&content)?;

    let spaces = raw
        .spaces
        .into_iter()
        .filter(|s| !s.id.is_empty() && !s.label.is_empty())
        .map(|s| SpaceItem {
            id: s.id,
            icon: s.icon,
            label: s.label,
            dot: s.dot,
            system: s.system.trim().to_string(),
            prompts: s
                .prompts
                .into_iter()
                .filter(|p| !p.label.is_empty() && !p.prompt.is_empty())
                .map(|p| PromptItem {
                    icon: p.icon,
                    label: p.label,
                    prompt: p.prompt,
                })
                .collect(),
            rag_enabled: None,
        })
        .collect();

    Ok(spaces)
}

pub fn save_spaces(path: &Path, spaces: Vec<SpaceItem>) -> Result<()> {
    let header = "\
# ============================================================\n\
#  Demeter – Configuration des espaces et prompts\n\
#\n\
#  Chaque \"espace\" correspond à un onglet dans la sidebar.\n\
#  Champs d'un espace :\n\
#    id          : identifiant unique (slug, pas d'espace)\n\
#    icon        : emoji affiché dans la sidebar\n\
#    label       : nom affiché dans la sidebar\n\
#    dot         : couleur du badge (green | blue | amber | \"\" pour aucun)\n\
#    system      : system prompt envoyé au LLM pour cet espace\n\
#    prompts     : liste de suggestions affichées sur l'écran d'accueil\n\
#\n\
#  Champs d'un prompt :\n\
#    icon   : emoji de la carte\n\
#    label  : titre court (< 22 caractères)\n\
#    prompt : texte envoyé au LLM au clic\n\
# ============================================================\n\n";

    let raw = YamlSpaces {
        spaces: spaces
            .into_iter()
            .map(|s| YamlSpace {
                id: s.id,
                icon: s.icon,
                label: s.label,
                dot: s.dot,
                system: s.system,
                prompts: s
                    .prompts
                    .into_iter()
                    .map(|p| YamlPrompt {
                        icon: p.icon,
                        label: p.label,
                        prompt: p.prompt,
                    })
                    .collect(),
            })
            .collect(),
    };

    let yml_body = serde_yaml::to_string(&raw)?;
    std::fs::write(path, format!("{}{}", header, yml_body))?;
    Ok(())
}
