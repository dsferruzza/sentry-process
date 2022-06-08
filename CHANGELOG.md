# Changelog

## 2.1.0

- Update dependencies (sentry: 0.20 → 0.26)

## 2.0.0

- Add a circular buffer to keep STDOUT/STDERR data below Sentry's size limit and truncate the oldest output first
- Update dependencies (sentry: 0.15 → 0.20)
- Change TLS implementation from OS native to Rustls
- Fix STDOUT/STDERR line endings on Windows
- Improve some error messages
- Various refactoring

## 1.0.1

- Update dependencies (sentry: 0.13 → 0.15)

## 1.0.0

Initial release
