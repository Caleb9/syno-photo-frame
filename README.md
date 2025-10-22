# Syno Photo Frame

[![Crates.io
Version](https://img.shields.io/crates/v/syno-photo-frame)](https://crates.io/crates/syno-photo-frame)

[Synology
Photos](https://www.synology.com/en-global/dsm/feature/photos) and
[Immich](https://immich.app/) full-screen slideshow for Raspberry Pi.

<img src="doc/syno-photo-frame.png" width=600 />

Features include speed control, transition effects, blurry background
fill, and date and location display.

<img src="doc/Slideshow.png" width=600 alt="Extra space gets blurry background" />

__If you like the project, give it a star ⭐, or consider becoming a__
[![](https://img.shields.io/static/v1?label=Sponsor&message=%E2%9D%A4&logo=GitHub&color=%23fe8e86)](https://github.com/sponsors/caleb9)
:)

- [Syno Photo Frame](#syno-photo-frame)
  - [Why?](#why)
  - [Setup](#setup)
    - [Synology Photos (NAS)](#synology-photos-nas)
        - [Limitations](#limitations)
    - [Alternative: Immich](#alternative-immich)
        - [Limitations](#limitations-1)
    - [Raspberry Pi](#raspberry-pi)
      - [Option 1: Install From Debian Package](#option-1-install-from-debian-package)
      - [Option 2: Build From Source](#option-2-build-from-source)
        - [Alternative: Build With Docker](#alternative-build-with-docker)
  - [Run](#run)
  - [Optional Stuff](#optional-stuff)
    - [Increase the Swap Size on Raspberry Pi Zero](#increase-the-swap-size-on-raspberry-pi-zero)
    - [Auto-start](#auto-start)
    - [Startup-Shutdown Schedule](#startup-shutdown-schedule)
    - [Auto Brightness](#auto-brightness)
    - [Start from a Random Photo and in Random Order](#start-from-a-random-photo-and-in-random-order)
    - [Change the Transition Effect](#change-the-transition-effect)
    - [Display Shooting Date and Location](#display-shooting-date-and-location)
    - [Customize the Splash-Screen](#customize-the-splash-screen)
  - [Disclaimer](#disclaimer)

## Why?

I developed this app for a DIY digital photo frame project using a
[Raspberry Pi](https://www.raspberrypi.com/) connected to a monitor
(it runs great on Pi Zero 2). The goal was to fetch photos directly
from my Synology NAS over LAN.

Why not use Synology Photos in a web browser directly? There are two
reasons. First, the current version of Synology Photos web app (1.8.x
at the time of writing) does not allow slideshow speed adjustments and
changes photos every 3 or 4 seconds - way too fast for a photo
frame. Second, running a full web browser is more resource-demanding
than a simple static image app, which matters when using a Raspberry
Pi, especially in the Zero variant.

## Setup

### Synology Photos (NAS)

Assuming the Synology Photos package is installed on DSM:

0. Create an __album__ in Synology Photos and add photos to it (note
   the distinction between an "album" and a "folder").
1. Click the "Share" button in the album.
2. Check the "Enable share link" option.
3. Copy/write down the Share Link - you'll need it when setting up the
   app on Raspberry Pi later on.
4. Set Privacy Settings to one of the "Public" options.
5. Optionally, enable Link Protection. If a password is set, you will
   need to provide it using the `--password` option when running the
   app on Raspberry Pi. In the case of accessing Synology Photos over
   the internet or an untrusted LAN, I recommend making sure your
   share link uses the HTTPS (not HTTP) scheme to prevent exposing the
   password.
6. Click Save.

<img src="doc/ShareLink.png" alt="Album Sharing" />

##### Limitations

* Accessing Synology Photos via a **Quick Connect** link is not
  currently supported.
* Upper limit on number of photos in an album is set to 5000.
* [Video playback is not
  supported](https://github.com/Caleb9/syno-photo-frame/issues/15)

### Alternative: Immich

Alternatively, instead of using Synology Photos, photos can also be
hosted on Immich server. Create an __album__ in Immich, add photos to
it, and create a share link (click the "Share" button in the
album). Optionally set a password (the same recommendation as with
Synology Photos about using HTTPS scheme when accessing the server
over the internet applies). Copy/write down the link - you'll need it
when setting up the app on Raspberry Pi later on.

##### Limitations

* Video playback is not supported

### Raspberry Pi

Let's assume that you're starting with a fresh installation of
Raspberry Pi OS Lite, the network has been set up (so you can access
Synology Photos or Immich server), and you can access the command line
on your Pi.

#### Option 1: Install From Debian Package

[Releases](https://github.com/Caleb9/syno-photo-frame/releases)
contains pre-built .deb packages for arm64 Linux architecture, which
should work on Raspberry Pi 3 and up, as well as Zero 2 (assuming the
64bit version of Raspbian OS *Bookworm* or newer is installed).

- Check the architecture with `dpkg --print-architecture`; it should
  print "arm64".
- Check the installed version of Debian with `lsb_release -c` and make
  sure it says "bookworm" or "trixie".

**For other platforms (including older versions of Debian, such as
"bullseye"), you must build the project from source - see [Option 2:
Build From Source](#option-2-build-from-source).**

1. Download the `syno-photo-frame_X.Y.Z_arm64.deb` package from
   Releases.
2. Update the system:

   ```bash
   sudo -- sh -c ' \
   apt update && \
   apt upgrade -y'
   ```

3. `cd` to the directory where the package has been downloaded and
   install the app:

   ```bash
   sudo apt install ./syno-photo-frame_*_arm64.deb
   ```

#### Option 2: Build From Source

Note: These instructions assume a Debian-based Linux distribution, but
adjusting them should make it possible to build the app for almost any
platform where Rust and [SDL](https://www.libsdl.org/) are available.

1. [Install Rust](https://www.rust-lang.org/tools/install) if you have
   not already (or use the [Alternative: Build With
   Docker](#alternative-build-with-docker) approach).

2. Install build dependencies:

   ```bash
   sudo -- sh -c ' \
   apt update && \
   apt upgrade -y && \
   apt install -y \
       libsdl2-dev \
       libsdl2-gfx-dev \
       libsdl2-ttf-dev \
       libssl-dev'
   ```

3. Install the app from
   [crates.io](https://crates.io/crates/syno-photo-frame) (you can use
   the same command to update the app when a new version gets
   published):

   ```bash
   cargo install syno-photo-frame
   ```

When building is finished, the binary is then located at
`$HOME/.cargo/bin/syno-photo-frame` and should be available on your
`$PATH`.

Alternatively, clone the git repository and build the project with (in
the cloned directory):

```bash
cargo build --release
```

The binary is then located at `target/release/syno-photo-frame`.

##### Alternative: Build With Docker

If you don't want to install Rust or the build dependencies for some
reason but have Docker available, you can build the binary and/or
Debian package in a container using the provided
[Dockerfile](Dockerfile). See instructions in the file to build the
app this way.

## Run

Display the help message to see various available options:

```bash
syno-photo-frame --help
```

Run the app:

```bash
syno-photo-frame {sharing link to Synology Photos or Immich album}
```

If everything works as expected, press Ctrl-C to kill the app.

## Optional Stuff

### Increase the Swap Size on Raspberry Pi Zero

A 100 MB swap file may be too small when running on low-memory systems
such as Pi Zero. See [Increasing Swap on a Raspberry
Pi](https://pimylifeup.com/raspberry-pi-swap-file/).

### Auto-start

To start the slideshow automatically on boot, you can add it to
crontab:

```bash
crontab -e
```

Add something like this at the end of crontab:

```bash
@reboot    sleep 5 && /bin/syno-photo-frame https://{share_link} >> /tmp/syno-photo-frame.log 2>&1
```

Remember to replace your share link with a real one and adjust the
binary path depending on the installation method (dpkg or from
crates.io). A short `sleep` is required to not start before some
services (network) are up - try to increase it if errors occur. The
above command redirects error messages to a log file
`/tmp/syno-photo-frame.log`.

For other (untested) alternatives, see e.g. [this
article](https://www.dexterindustries.com/howto/run-a-program-on-your-raspberry-pi-at-startup/).

### Startup-Shutdown Schedule

A proper digital photo frame doesn't run 24/7. Shutdown can be
scheduled in software only, but for startup, you'll need a hardware
solution, e.g. for Raspberry Pi Zero, I'm using [Witty Pi 4
Mini](https://www.uugear.com/product/witty-pi-4-mini/).

### Auto Brightness

For my digital photo frame project, I attached a light sensor to Pi's
GPIO to adjust the monitor's brightness automatically depending on
ambient light. [TSL2591](https://www.adafruit.com/product/1980) is an
example of such sensor. Check out my
[auto-brightness-rpi-tsl2591](https://github.com/Caleb9/auto-brightness-rpi-tsl2591)
project to add automatic brightness control to your digital photo
frame.

### Start from a Random Photo and in Random Order

By default, photos are displayed in the order of the shooting date. If
the album is very large, and the startup-shutdown schedule is short,
potentially the slideshow might never reach some of the later photos
in the album. The `--random-start` option solves this problem by
starting the slideshow at a randomly selected photo, then continuing
normally (in the order of the shooting date). Adding this option to
the startup schedule will start at a different photo every time.

Alternatively, use `--order random` to display photos in a completely
random order.

### Change the Transition Effect

Use the `--transition` (or `-t`) option to select the type of
transition effect for changing photos. Use `--help` option to display
valid values.

### Display Shooting Date and Location

Use the `--display-photo-info` to show shooting date and location (if
available) on screen. This setting is off by default.

The date format is controlled by operating system's locale: `LC_ALL`,
`LC_TIME`, or `LANG` environment variable, in order of descending
priority. You can prepend `syno-photo-frame` command with the
variable, e.g.

```bash
LC_ALL="en_GB.UTF-8" syno-photo-frame ...
```

Find out list of installed locales with `locale -a`.

### Customize the Splash-Screen

You can replace the default image displayed during loading of the
first photo. Use the `--splash` option to point the app to a .jpeg
file location.

## Disclaimer

This project is an independent, open-source application and is not
affiliated, associated, authorized, endorsed by, or in any way
officially connected with Synology Inc. "Synology" and any related
product names, logos, and trademarks are the property of Synology Inc.

The use of Synology Photos in this project is solely for
interoperability purposes, and the project does not provide any
official support from Synology. All trademarks, product names, and
company names mentioned in this repository belong to their respective
owners.
