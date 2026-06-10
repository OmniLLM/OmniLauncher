use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) const SKILLS_CONFIG_DIR_ENV: &str = "OMNILAUNCHER_SKILLS_CONFIG_DIR";

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn skills_config_dir() -> PathBuf {
    if let Ok(path) = std::env::var(SKILLS_CONFIG_DIR_ENV) {
        return PathBuf::from(path);
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("skills")
}

pub fn credential_env_for_skill(skill: &str) -> HashMap<String, String> {
    if skill != "gcp" {
        return HashMap::new();
    }

    let config_dir = skills_config_dir();
    let credential_path = config_dir.join("credential.json");
    let Ok(raw) = std::fs::read_to_string(&credential_path) else {
        log::debug!(
            "skill_credentials: no credential profile at {}",
            credential_path.display()
        );
        return HashMap::new();
    };
    let Ok(profile) = serde_json::from_str::<Value>(&raw) else {
        log::warn!(
            "skill_credentials: failed to parse credential profile at {}",
            credential_path.display()
        );
        return HashMap::new();
    };
    let Some(service_account_key) = profile
        .get("cloud")
        .and_then(|v| v.get("gcp"))
        .and_then(|v| v.get("gcp"))
        .and_then(|v| v.get("service_account_key"))
        .filter(|v| v.is_object())
    else {
        log::debug!("skill_credentials: no inline GCP service_account_key found");
        return HashMap::new();
    };

    if std::fs::create_dir_all(&config_dir).is_err() {
        log::warn!(
            "skill_credentials: failed to create skills config dir {}",
            config_dir.display()
        );
        return HashMap::new();
    }

    let adc_path = config_dir.join("gcp_sa_key.json");
    let Ok(rendered) = serde_json::to_string_pretty(service_account_key) else {
        log::warn!("skill_credentials: failed to serialize GCP service account key");
        return HashMap::new();
    };
    if let Err(err) = std::fs::write(&adc_path, rendered) {
        log::warn!(
            "skill_credentials: failed to write ADC file {}: {err}",
            adc_path.display()
        );
        return HashMap::new();
    }

    let mut env = HashMap::new();
    env.insert(
        "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
        adc_path.to_string_lossy().into_owned(),
    );
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn gcp_inline_service_account_key_is_materialized_as_adc_env() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let config = TempDir::new().unwrap();
        std::env::set_var(SKILLS_CONFIG_DIR_ENV, config.path());

        let service_account_key = serde_json::json!({
            "type": "service_account",
            "project_id": "example-project",
            "private_key_id": "dummy",
            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
            "client_email": "example@example-project.iam.gserviceaccount.com",
            "client_id": "1234567890"
        });
        let profile = serde_json::json!({
            "cloud": {
                "gcp": {
                    "gcp": {
                        "service_account_key": service_account_key,
                        "default_scope": "organizations/563352322117"
                    }
                }
            }
        });
        fs::write(config.path().join("credential.json"), profile.to_string()).unwrap();

        let env = credential_env_for_skill("gcp");
        std::env::remove_var(SKILLS_CONFIG_DIR_ENV);

        let adc = config.path().join("gcp_sa_key.json");
        assert_eq!(
            env.get("GOOGLE_APPLICATION_CREDENTIALS"),
            Some(&adc.to_string_lossy().into_owned())
        );

        let written: Value = serde_json::from_str(&fs::read_to_string(adc).unwrap()).unwrap();
        assert_eq!(
            written["client_email"],
            "example@example-project.iam.gserviceaccount.com"
        );
        assert_eq!(written["project_id"], "example-project");
    }
}
