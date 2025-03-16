# FTP Photo Frame

FTP-connected Full-screen slideshow for Raspberry Pi based on [Syno-Photo-Frame](https://github.com/Caleb9/syno-photo-frame).

Features include speed control, transition effects, and a blurry
background fill.

__This is a heavily stripped-down version of a much better tested and more connected Project: [Syno-Photo-Frame](https://github.com/Caleb9/syno-photo-frame)</br>
Consider that instead, if you utilize one of it's integrations.__

- [Why?](#why)
- [Setup](#setup)
    - [Build with Docker](#build-with-docker)
    - [Motionsensor](#motionsensor)
- [Run](#run)
    - [CLI Options](#cli-options)
- [Optional Stuff](#optional-stuff)
  - [Increase the Swap Size on Raspberry Pi Zero](#increase-the-swap-size-on-raspberry-pi-zero)
  - [Auto-start](#auto-start)
  - [Startup-Shutdown Schedule](#startup-shutdown-schedule)
  - [Auto Brightness](#auto-brightness)
  - [Start from a Random Photo and in Random Order](#start-from-a-random-photo-and-in-random-order)
  - [Change the Transition Effect](#change-the-transition-effect)
  - [Customize the Splash-Screen](#customize-the-splash-screen)

## Why?

I wanted to have a digital photo frame under my full control and accessing my photos stored on my NAS. Luckily I found [Syno-Photo-Frame](https://github.com/Caleb9/syno-photo-frame), which matched many of my needs. However, at the time, the Synology Photos API must've changed just before my first attempts at utilizing it. I struggled debugging the problem, since I have almost no knowledge of internet based APIs...

Hence I decided to switch to a different method of obtaining the photos, 
which quickly brought me to FTP. If kept in a local network it is usually safe and it is a pretty simple protocol to implement.</br>
I also wanted to add a motionsensor based standby into the software.</br>

Since doing this meant restructuring many of the main software components, I created this fork to tinker with. Due to my lack of time and skill I can't upkeep the high quality of the original project, so please consider using [it](https://github.com/Caleb9/syno-photo-frame) instead.

## Setup

### Build with Docker

If you don't want to install Rust or the build dependencies for some
reason but have Docker available, you can build the binary and/or
Debian package in a container using the provided
[Dockerfile](./docker/Dockerfile). See instructions in the file to build the
app this way.

### Motionsensor

With the motionsensor option you can use a simple signal on a GPIO pin to attempt to move your display into standby mode via an HDMI call, if your monitor supports it.

#### Requirements
- You need to use `dtoverlay=vc4-fkms-v3d` in `/boot/firmware/config.txt` on Raspbian

The Pin is currently hardcoded to GPIO23 (*planned to be configurable*) and the software expects a high signal for 'presence detected'. If no presence has been detected for a while, it will attempt to signal standby to your HDMI display and afterwards wake it up as soon as a presence has been detected.

*__TODO: Pin configuration etc.__*

## Run

#### CLI Options
- `--server` FTP-Server IP or Hostname
- `--folder` Images folder path
- `--user` FTP Username
- `--password` FTP Password
- `-i` photo change interval in seconds (default 30s)
- `-o` display order: `random`, `byname`, `bydate`(default)
- `--random-start`
- `--motion-sensor` activates the standby functionality. Refer to [Motionsensor](#motionsensor)
- `--rotation` rotation of the display in degrees
- `--splash` Path to custom JPEG splashscreen

*__TODO: Motion-sensor, parameters__*

If everything works as expected, press Ctrl-C to kill the app.

## Optional Stuff

### Increase the Swap Size on Raspberry Pi Zero

A 100 MB swap file may be too small when running on low-memory systems
such as Pi Zero. See [Increasing Swap on a Raspberry
Pi](https://pimylifeup.com/raspberry-pi-swap-file/).

### Auto-start

To start the slideshow automatically on boot, you can add it to
crontab. </br>
Directly add a startup call there or instead add a script containing the startup call:
```bash
#!/bin/bash
./ftp-photo-frame --server=192.168.178.69 --folder="/home/Photos/Gallery" --user=MYUSERNAME -p=MYPASSWORD -i=20 -o=random --motionsensor
```
Then open crontab
```bash
crontab -e
```

Add something like this at the end of crontab, save and exit:

```bash
@reboot ~/start.sh
```

For other (untested) alternatives, see e.g. [this
article](https://www.dexterindustries.com/howto/run-a-program-on-your-raspberry-pi-at-startup/).

### Startup-Shutdown Schedule

A proper digital photo frame doesn't run 24/7. Shutdown can be
scheduled in software only, but for startup, you'll need a hardware
solution, e.g. for Raspberry Pi Zero  [Witty Pi 3
Mini](https://www.adafruit.com/product/5038) appears to be an option.



### Auto Brightness

For the digital photo frame project, @Caleb9 attached a light sensor to Pi's
GPIO to adjust the monitor's brightness automatically depending on
ambient light. [TSL2591](https://www.adafruit.com/product/1980) is an
example of such sensor. Check out @Caleb9 's
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

### Customize the Splash-Screen

You can replace the default image displayed during loading of the
first photo. Use the `--splash` option to point the app to a .jpeg
file location.
