# Building Debian Package

To build a .deb package execute:

```
make VERSION=0.1.2
```

Remember to replace version number with appropriate value, and to
update `changelog`.

**The script assumes it is executed on `arm64` architecture**. It
updates version number in `Cargo.toml` so that executing the app with
`--version` option prints correct value. Makefile also uses `lintian`
tool to lint the resulting package - install with `apt install
lintian`.