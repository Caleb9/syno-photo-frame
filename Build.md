# Build Instructions

Follow these steps to build the project:

**Run the build command for cross-rs**:
```sh
cross build --target "aarch64-unknown-linux-gnu"
```

or

**Run the build command for docker**:
```sh
docker run --rm -v ${pwd}:/build -w /build rust-sdl2-aarch64 /bin/bash -c "source $HOME/.cargo/env; cargo build --target=aarch64-unknown-linux-gnu"
```
