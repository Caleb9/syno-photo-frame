# The script assumes building on arm64 linux platform

# Extract package version from Cargo.toml unless specified as
# environment variable
VERSION?=$(shell sed -n 's/^version = "\(\S*\)"$$/\1/p' ../Cargo.toml)

name=syno-photo-frame
arch=arm64

dpkg_name=$(name)_$(VERSION)_$(arch)

all:
# Set version in Cargo.toml, note that this can leave the git worktree in a dirty state
	@sed -i 's/^version = "\(..*\)"/version = "$(VERSION)"/' ../Cargo.toml
	@cargo build --release
# Create package build dir hierarchy
	@mkdir -p $(dpkg_name)/bin
	@mkdir -p $(dpkg_name)/DEBIAN
	@cp control $(dpkg_name)/DEBIAN/
	@mkdir -p $(dpkg_name)/usr/share/doc/$(name)
	@cp copyright $(dpkg_name)/usr/share/doc/$(name)
# Set version in package control file
	@sed -i 's/^Version: 0.0.0/Version: $(VERSION)/' $(dpkg_name)/DEBIAN/control
# Trim the binary
	@objcopy --strip-debug --strip-unneeded ../target/release/$(name) $(dpkg_name)/bin/$(name)
	@gzip -c --best -k -n changelog > $(dpkg_name)/usr/share/doc/$(name)/changelog.gz
	@dpkg-deb --root-owner-group --build $(dpkg_name)
# `lintian` needs to be installed for this to work
	@lintian $(dpkg_name).deb
# Clean up build directory
	@rm -r $(dpkg_name)

clean:
	@rm -rvf \
		$(name)_*_$(arch) \
		*.deb
