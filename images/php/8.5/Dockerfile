# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e

FROM dunglas/frankenphp@sha256:99a8142b702d9387682b3c845ae21f41ac611961aa20237357e73c93acee6ad2

COPY extensions.txt /tmp/stackctl-php-extensions.txt
COPY install-specs.txt /tmp/stackctl-php-install-specs.txt

RUN sed -i \
        -e 's|http://deb.debian.org/debian-security|http://snapshot.debian.org/archive/debian-security/20260713T000000Z|g' \
        -e 's|http://deb.debian.org/debian|http://snapshot.debian.org/archive/debian/20260713T000000Z|g' \
        /etc/apt/sources.list.d/debian.sources \
    && printf 'Acquire::Check-Valid-Until "false";\n' \
        > /etc/apt/apt.conf.d/99stackctl-snapshot \
    && xargs install-php-extensions < /tmp/stackctl-php-install-specs.txt \
    && while IFS= read -r extension; do \
        find /usr/local/etc/php/conf.d \
            -type f \
            -name "*${extension}*.ini" \
            -delete; \
    done < /tmp/stackctl-php-extensions.txt \
    && rm \
        /tmp/stackctl-php-extensions.txt \
        /tmp/stackctl-php-install-specs.txt

LABEL org.opencontainers.image.source="https://github.com/faustbrian/stackctl"
LABEL org.opencontainers.image.description="Stackctl v8 PHP 8.5 runtime"
