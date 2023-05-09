## Building

Building with `use-pkgconfig, static-link` SDL2 features requires:
```
libsdl2-dev
libdrm-dev
libgbm-dev
```
Note that this doesn't seem to work as the binary complains when `libsdl2-2.0-0` is not installed.

Building with dynamically linked SDL2 (without `static-link, use-pkgconfig`) requires only `libsdl2-dev`

For targets other than `aarch64-unknown-linux-gnu` `libssl-dev` is also needed.


## Running
```
libsdl2-2.0-0
libgl1
libegl1
```


## Auto-start

In `/etc/rc.local`:
```
/home/pi/syno-photo-frame https://{SHARE_LINK} >> /tmp/syno-photo-frame.log 2>&1 &
```
