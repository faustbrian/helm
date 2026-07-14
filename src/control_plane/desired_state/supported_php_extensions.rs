pub(crate) const SUPPORTED_PHP_EXTENSIONS: &[&str] = &[
    "bcmath",
    "exif",
    "gd",
    "imagick",
    "intl",
    "pcntl",
    "pcov",
    "pdo_mysql",
    "pdo_pgsql",
    "redis",
    "sockets",
    "sodium",
    "xdebug",
    "zip",
];

pub(crate) fn supports_php_extension(extension: &str) -> bool {
    SUPPORTED_PHP_EXTENSIONS.binary_search(&extension).is_ok()
}

#[cfg(test)]
mod tests {
    use super::SUPPORTED_PHP_EXTENSIONS;

    #[test]
    fn published_php_image_installs_the_supported_extension_catalog() {
        let published = include_str!("../../../images/php/8.5/extensions.txt")
            .lines()
            .collect::<Vec<_>>();
        let installation_specs = include_str!("../../../images/php/8.5/install-specs.txt")
            .lines()
            .map(|specification| {
                specification
                    .rsplit_once('-')
                    .filter(|(_, version)| {
                        version.starts_with(|value: char| value.is_ascii_digit())
                    })
                    .map_or(specification, |(extension, _)| extension)
            })
            .collect::<Vec<_>>();

        assert_eq!(published, SUPPORTED_PHP_EXTENSIONS);
        assert_eq!(installation_specs, SUPPORTED_PHP_EXTENSIONS);
    }
}
