//! Lockfile diffing helpers.

use std::collections::BTreeMap;

use crate::config::{LockedImage, Lockfile};

pub struct LockfileDiff {
    pub missing: Vec<LockedImage>,
    pub changed: Vec<(LockedImage, LockedImage)>,
    pub extra: Vec<LockedImage>,
}

pub fn lockfile_diff(expected: &Lockfile, actual: &Lockfile) -> LockfileDiff {
    let expected_map = by_service(expected);
    let actual_map = by_service(actual);

    let mut missing = Vec::new();
    let mut changed = Vec::new();
    let mut extra = Vec::new();

    for (service, expected_image) in &expected_map {
        match actual_map.get(service) {
            None => missing.push(expected_image.clone()),
            Some(actual_image)
                if actual_image.image != expected_image.image
                    || actual_image.resolved != expected_image.resolved =>
            {
                changed.push((expected_image.clone(), actual_image.clone()));
            }
            Some(_) => {}
        }
    }

    for (service, actual_image) in &actual_map {
        if !expected_map.contains_key(service) {
            extra.push(actual_image.clone());
        }
    }

    LockfileDiff {
        missing,
        changed,
        extra,
    }
}

fn by_service(lockfile: &Lockfile) -> BTreeMap<String, LockedImage> {
    lockfile
        .images
        .iter()
        .map(|entry| (entry.service.clone(), entry.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::lockfile_diff;
    use crate::config::{LockedImage, Lockfile};

    fn locked(service: &str, image: &str, resolved: &str) -> LockedImage {
        LockedImage {
            service: service.to_owned(),
            image: image.to_owned(),
            resolved: resolved.to_owned(),
        }
    }

    #[test]
    fn lockfile_diff_reports_missing_changed_and_extra_entries() {
        let expected = Lockfile {
            version: 1,
            images: vec![
                locked("app", "ghcr.io/acme/app:latest", "sha-app"),
                locked("db", "mysql:8.4", "sha-db"),
            ],
        };

        let actual = Lockfile {
            version: 1,
            images: vec![
                locked("app", "ghcr.io/acme/app:main", "sha-app"),
                locked("search", "meilisearch:v1", "sha-search"),
                locked("db", "mysql:8.4", "sha-db"),
            ],
        };

        let diff = lockfile_diff(&expected, &actual);

        assert_eq!(diff.missing.len(), 0);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].0.service, "app");
        assert_eq!(diff.extra.len(), 1);
        assert_eq!(diff.extra[0].service, "search");
        assert_eq!(diff.changed[0].1.image, "ghcr.io/acme/app:main");
    }

    #[test]
    fn lockfile_diff_detects_versioned_none_values() {
        let expected = Lockfile {
            version: 1,
            images: vec![LockedImage {
                service: "db".to_owned(),
                image: "mysql:8.4".to_owned(),
                resolved: String::new(),
            }],
        };
        let actual = Lockfile {
            version: 1,
            images: vec![LockedImage {
                service: "db".to_owned(),
                image: "mysql:8.4".to_owned(),
                resolved: "sha-app".to_owned(),
            }],
        };

        let diff = lockfile_diff(&expected, &actual);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.missing.len(), 0);
        assert!(diff.extra.is_empty());
    }
}
