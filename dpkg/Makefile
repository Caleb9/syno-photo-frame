VERSION?=0.0.0

name=syno-photo-frame
arch=arm64

target=$(name)_$(VERSION)_$(arch)

all:
	@sed -i 's/^version = "\(..*\).\(..*\).\(..*\)"/version = "$(VERSION)"/' ../Cargo.toml
	@cargo build --release
	@cp -r $(name)_$(arch) $(target)
	@sed -i 's/^Version: 0.0.0/Version: $(VERSION)/' $(target)/DEBIAN/control
	@objcopy --strip-debug --strip-unneeded ../target/release/$(name) $(target)/bin/$(name)
	@gzip -c --best -k -n changelog > $(target)/usr/share/doc/$(name)/changelog.gz
	@dpkg-deb --root-owner-group --build $(target)
# lintian needs to be installed
	@lintian $(target).deb
	@rm -r $(target)

clean:
	@rm -rvf \
		$(name)_*_$(arch) \
		*.deb
