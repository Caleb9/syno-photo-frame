# Syno Photo Frame

[Synology
Photos](https://www.synology.com/en-global/dsm/feature/photos)
full-screen slideshow for Raspberry Pi.

![](doc/syno-photo-frame.png)

Features speed control, transition effects and blurry background fill.

![](doc/Slideshow.png "Extra space gets blurry background")

__If you like the project, give it a star ⭐, or consider becoming a__
[![](https://img.shields.io/static/v1?label=Sponsor&message=%E2%9D%A4&logo=GitHub&color=%23fe8e86)](https://github.com/sponsors/caleb9)
:)

- [Syno Photo Frame](#syno-photo-frame)
  - [Why?](#why)
    - [Security Disclaimer](#security-disclaimer)
  - [Setup](#setup)
    - [Synology Photos (NAS)](#synology-photos-nas)
    - [Raspberry Pi](#raspberry-pi)
      - [Install From Debian Package](#install-from-debian-package)
      - [Alternative: Build From Source](#alternative-build-from-source)
  - [Run](#run)
  - [Optional Stuff](#optional-stuff)
    - [Increase swap size on Raspberry Pi Zero](#increase-swap-size-on-raspberry-pi-zero)
    - [Auto-start](#auto-start)
    - [Startup-Shutdown Schedule](#startup-shutdown-schedule)
    - [Start Slideshow From Random Photo and Random Order](#start-slideshow-from-random-photo-and-random-order)
    - [Change Transition Effect](#change-transition-effect)
    - [Customize splash-screen](#customize-splash-screen)
    - [Auto Brightness](#auto-brightness)
  - [Supported By](#supported-by)

## Why?

I wrote this app for a DIY digital photo frame project using
[Raspberry Pi](https://www.raspberrypi.com/) connected to a monitor
(runs great on Pi Zero 2). The goal was to fetch photos directly from
my Synology NAS over LAN.

Why not use Synology Photos in a web browser directly? There are two
reasons. First, current version of Synology Photos (1.6.1 at the time
of writing) does not allow slideshow speed adjustments, and changes
photo every 3 or 4 seconds - way too fast for a photo frame. Second,
running a full www browser is more resource demanding than a simple
static image app, which matters when using Raspberry Pi, especially in
the Zero variant.


### Security Disclaimer

Since I am running all of this over local area network at home only,
security is not a priority for me. You should take extra steps which
are out of scope of this guide if you plan to access your NAS over
Internet, or if you're running this setup in an un-trusted LAN
(e.g. in an office or a dorm).


## Setup

### Synology Photos (NAS)

Assuming Synology Photos package is installed on DSM

0. Create an __album__ in Synology Photos and add photos to it (note
   the distinction between an "album" and a "folder")
1. Click "Share" button in the album
2. Check "Enable share link" option
3. Copy the Share Link somewhere - you'll need it when setting up the
   app on Raspberry Pi later on
4. Set Privacy Settings to "Public - Anyone with the link can view"
5. __Do NOT enable Link Protection__ - support for password protection
   might get implemented in the future, for now the assumption is that
   the NAS and Pi are on the same *private* LAN.
6. Click Save

![Share Album](doc/ShareLink.png)


### Raspberry Pi

The assumption is that you are starting with a fresh installation of
Raspberry Pi OS Lite, network has been set up (so you can access the
Synology NAS) and you have command line access to the Pi.


#### Install From Debian Package

Update the system

```
sudo -- sh -c ' \
apt update && \
apt upgrade -y'
```

[Releases](https://github.com/Caleb9/syno-photo-frame/releases)
contains pre-built .deb packages for aarch64 Linux target, which
should work on Raspberry Pi 3 and up, as well as Zero 2 (assuming
64bit version of Raspbian OS is installed). For other platforms you
must build the project yourself - see [Alternative: Build From
Source](#alternative-build-from-source).

Check the installed version of Debian with `lsb_release -c` and
download appropriate
`syno-photo-frame_X.Y.Z_arm64_{debian_codename}.deb` package from
Releases. Note that currently the only available packages are for
Debian 11 ("bullseye") and 12 ("bookworm").

`cd` to directory where the package has been downloaded and install
the app (adjust the filename appropriately):

```
sudo apt install ./syno-photo-frame_0.7.0_arm64_bookworm.deb
```


#### Alternative: Build From Source

[Install Rust](https://www.rust-lang.org/tools/install) if you have
not already.

Install build dependencies:
```
sudo -- sh -c ' \
apt update && \
apt upgrade -y && \
apt install -y \
	libsdl2-dev \
	libsdl2-ttf-dev \
	libssl-dev'
```

Install the app from [crates.io](https://crates.io/crates/syno-photo-frame):
```
cargo install syno-photo-frame
```

The binary is then located at `$HOME/.cargo/bin/syno-photo-frame` and
should be available on your `$PATH`.

Alternatively, clone the git repository and build the project with:
```
cargo build --release
```

The binary is then located at `target/release/syno-photo-frame`.


## Run

Display help message:
```
syno-photo-frame --help
```

Run the app:
```
syno-photo-frame {sharing link to Synology Photos album}
```

If everything works as expected, press Ctrl-C to kill the app.


## Optional Stuff

### Increase swap size on Raspberry Pi Zero

100 MB swap file may be too small when running on low memory systems
such as Pi Zero. See [Increasing Swap on a Raspberry
Pi](https://pimylifeup.com/raspberry-pi-swap-file/).


### Auto-start

To start the slideshow automatically on boot, you can add it to crontab:
```
crontab -e
```
Add something like this at the end of crontab:
```
@reboot    sleep 5 && /bin/syno-photo-frame https://{share_link} >> /tmp/syno-photo-frame.log 2>&1
```

Remember to replace your share link with a real one, and adjust the
binary path depending on installation method (dpkg or from
crates.io). Short `sleep` is required to not start before some
services (network) are up - try to increase it if errors occur. The
above command redirects error messages to a log file
`/tmp/syno-photo-frame.log`.

For other (untested) alternatives see e.g. [this
article](https://www.dexterindustries.com/howto/run-a-program-on-your-raspberry-pi-at-startup/).


### Startup-Shutdown Schedule

A proper digital photo frame doesn't run 24/7. Shutdown can be
scheduled in software only, but for startup you'll need a hardware
solution, e.g. for Raspberry Pi Zero I'm using [Witty Pi 3
Mini](https://www.adafruit.com/product/5038).


### Start Slideshow From Random Photo and Random Order

By default photos are displayed in the order of shooting date. If the
album is very large, and the startup-shutdown schedule is short,
potentially the slideshow might never reach some of the later photos
in the album. The `--order random-start` option solves the problem by
starting the slideshow at randomly selected photo, then continuing
normally (in order of shooting date). Adding this option to the
startup schedule will start at a different photo every time.

Alternatively use `--order random` to display photos in completely
random order.


### Change Transition Effect

Use the `--transition` (or `-t`) option to select type of transition
effect for changing photos. Use `--help` option to display valid
values.


### Customize splash-screen

You can replace the default image displayed during loading of first
photo. Use the `--splash` option to point the app to a .jpeg file
location.


### Auto Brightness

For my digital photo frame project I attached a light sensor to Pi's
GPIO to adjust monitor's brightness automatically depending on ambient
light. [TSL2591](https://www.adafruit.com/product/1980) is an example
of such sensor. Check out my
[auto-brightness-rpi-tsl2591](https://github.com/Caleb9/auto-brightness-rpi-tsl2591)
project to add automatic brightness control to your digital photo
frame.


## Supported By

[![JetBrains Logo (Main)
logo](https://resources.jetbrains.com/storage/products/company/brand/logos/jb_beam.svg)](https://jb.gg/OpenSourceSupport)
