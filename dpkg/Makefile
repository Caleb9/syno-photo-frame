# Extract package version from Cargo.toml unless specified as
# environment variable
VERSION?=$(shell sed -n 's/^version = "\(\S*\)"$$/\1/p' ../Cargo.toml)

arch=$(shell dpkg --print-architecture)
name=syno-photo-frame
dpkg_name=$(name)_$(VERSION)_$(arch)

all:
# Set version in Cargo.toml, note that this can leave the git worktree in a dirty state
	@sed -i 's/^version = "\(..*\)"/version = "$(VERSION)"/' ../Cargo.toml
	@cargo build --release
# Create package build dir hierarchy
	@mkdir -m 0755 $(dpkg_name)
	@mkdir -m 0755 $(dpkg_name)/bin
	@mkdir -m 0755 $(dpkg_name)/usr
	@mkdir -m 0755 $(dpkg_name)/usr/share
	@mkdir -m 0755 $(dpkg_name)/usr/share/doc
	@mkdir -m 0755 $(dpkg_name)/usr/share/doc/$(name)
	@mkdir $(dpkg_name)/DEBIAN
	@cp control $(dpkg_name)/DEBIAN/
	@cp -p copyright $(dpkg_name)/usr/share/doc/$(name)
# Set version and architecture in package control file
	@sed -i 's/^Version: 0.0.0/Version: $(VERSION)/' $(dpkg_name)/DEBIAN/control
	@sed -i 's/^Architecture:/Architecture: $(arch)/' $(dpkg_name)/DEBIAN/control
# Trim the binary
	@objcopy --strip-debug --strip-unneeded ../target/release/$(name) $(dpkg_name)/bin/$(name)
	@chmod 0755 $(dpkg_name)/bin/$(name)
	@gzip -c --best -k -n changelog > $(dpkg_name)/usr/share/doc/$(name)/changelog.gz
	@chmod 0644 $(dpkg_name)/usr/share/doc/$(name)/changelog.gz
	@dpkg-deb --root-owner-group --build $(dpkg_name)
# `lintian` needs to be installed for this to work
	@lintian $(dpkg_name).deb
# Generate SHA256 checksum file
	@sha256sum $(dpkg_name).deb > $(dpkg_name).sha256
# Clean up build directory
	@rm -r $(dpkg_name)

clean:
	@rm -rvf \
		$(name)_*_$(arch) \
		*.deb
