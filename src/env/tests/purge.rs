use super::super::write_env_values_with_purge;
use super::helpers::temp_env_file;
use std::collections::{HashMap, HashSet};
use std::fs;

#[test]
fn write_env_values_with_purge_removes_stale_managed_vars() -> anyhow::Result<()> {
    crate::docker::with_dry_run_state(false, || -> anyhow::Result<()> {
        let env_path = temp_env_file("purge");
        fs::write(
            &env_path,
            "APP_URL=\"https://old\"\nASSET_URL=\"https://old\"\nPAP_URL=\"https://old\"\nFOO=\"bar\"\n",
        )?;

        let mut values = HashMap::new();
        values.insert("APP_URL".to_owned(), "https://new".to_owned());

        let managed_keys = HashSet::from([
            "APP_URL".to_owned(),
            "ASSET_URL".to_owned(),
            "PAP_URL".to_owned(),
        ]);

        write_env_values_with_purge(&env_path, &values, true, &managed_keys, true)?;
        let updated = fs::read_to_string(&env_path)?;
        fs::remove_file(&env_path)?;

        assert!(updated.contains("APP_URL=\"https://new\""));
        assert!(!updated.contains("ASSET_URL"));
        assert!(!updated.contains("PAP_URL"));
        assert!(updated.contains("FOO=\"bar\""));
        Ok(())
    })
}

#[test]
fn write_env_values_with_purge_keeps_missing_when_purge_disabled() -> anyhow::Result<()> {
    crate::docker::with_dry_run_state(false, || -> anyhow::Result<()> {
        let env_path = temp_env_file("no-purge");
        fs::write(
            &env_path,
            "APP_URL=\"https://old\"\nASSET_URL=\"https://old\"\nFOO=\"bar\"\n",
        )?;

        let mut values = HashMap::new();
        values.insert("APP_URL".to_owned(), "https://new".to_owned());

        let managed_keys = HashSet::from(["APP_URL".to_owned(), "ASSET_URL".to_owned()]);

        write_env_values_with_purge(&env_path, &values, true, &managed_keys, false)?;
        let updated = fs::read_to_string(&env_path)?;
        fs::remove_file(&env_path)?;

        assert!(updated.contains("APP_URL=\"https://new\""));
        assert!(updated.contains("ASSET_URL=\"https://old\""));
        assert!(updated.contains("FOO=\"bar\""));
        Ok(())
    })
}
