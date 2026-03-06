//! cli args arg enums module.
//!
//! Contains cli args arg enums logic used by Helm command workflows.

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum PullPolicyArg {
    Always,
    Missing,
    Never,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum PackageManagerArg {
    Bun,
    Npm,
    Pnpm,
    Yarn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PortStrategyArg {
    Random,
    Stable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ShareProviderArg {
    Cloudflare,
    Expose,
    Tailscale,
}

#[cfg(test)]
mod tests {
    use super::{PackageManagerArg, PortStrategyArg, PullPolicyArg, ShareProviderArg};
    use clap::ValueEnum;

    #[test]
    fn package_manager_arg_has_expected_variants() {
        assert_eq!(
            PackageManagerArg::value_variants().len(),
            4,
            "package manager enum should expose four CLI values",
        );

        let names = PackageManagerArg::value_variants()
            .into_iter()
            .filter_map(|value| value.to_possible_value())
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["bun", "npm", "pnpm", "yarn"]);
    }

    #[test]
    fn pull_policy_arg_is_comparable_and_complete() {
        let pull_values = PullPolicyArg::value_variants()
            .iter()
            .filter_map(|value| value.to_possible_value())
            .count();
        let port_values = PortStrategyArg::value_variants()
            .iter()
            .filter_map(|value| value.to_possible_value())
            .count();

        assert_eq!(pull_values, 3);
        assert_eq!(port_values, 2);
    }

    #[test]
    fn share_provider_arg_has_known_values() {
        let values = ShareProviderArg::value_variants()
            .into_iter()
            .filter_map(|value| value.to_possible_value())
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();

        assert!(values.contains(&"cloudflare".to_owned()));
        assert!(values.contains(&"expose".to_owned()));
        assert!(values.contains(&"tailscale".to_owned()));
    }
}
