# Building Debian Package

To build a .deb package with version specified in `Cargo.toml`
execute:

```
make
```

Optionally, version can be specified as environment variable:

```
make VERSION=0.1.2
```

Remember to update `changelog` when building a new version.

The script updates version number in `Cargo.toml` (so that executing
the app with `--version` option prints the correct value) and
therefore it may leave the repository in a dirty state (if version
different from the one in `Cargo.toml` was specified as environment
variable). Also, Makefile uses `lintian` tool to lint the resulting
package - install it with

```
apt install lintian
```
