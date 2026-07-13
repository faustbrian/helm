/// Supported PHP project tools available inside an application runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PhpTool {
    PhpStan,
    Ecs,
    PhpCsFixer,
    Psalm,
    Pint,
    Pest,
    PhpUnit,
    Rector,
}

impl PhpTool {
    pub(super) const fn executable(self) -> &'static str {
        match self {
            Self::PhpStan => "phpstan",
            Self::Ecs => "ecs",
            Self::PhpCsFixer => "php-cs-fixer",
            Self::Psalm => "psalm",
            Self::Pint => "pint",
            Self::Pest => "pest",
            Self::PhpUnit => "phpunit",
            Self::Rector => "rector",
        }
    }
}
