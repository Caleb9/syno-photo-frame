## Set framebuffer to 32bpp depth

Add the following to `/boot/cmdline.txt`:

```
video=HDMI-A-1:1920x1080M-32@60
```

## Startup

`startSlideshow.sh`:

```
#!/bin/sh

if [ -z "$SLIDESHOW" ]; then
    export SLIDESHOW=1
    /home/pi/auto-brightness-rpi-tsl2591/auto-brightness-rpi-tsl2591.pex >> /tmp/auto-brightness-rpi-tsl25591.log 2>&1 &
    /home/pi/syno-photo-frame https://nas.piotrkarasinski.eu/photo/mo/sharing/Wd36CZH2H >> /tmp/synology-photos-slideshow.log 2>&1
fi
```

Add to `. startSlideshow.sh` to `.profile` (?)
