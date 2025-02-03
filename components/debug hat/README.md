# Debug Add-on Board

Raspberry Pi Pico W with MicroPython `v1.22.2`.

To install the application and its `cbor2` dependency install
[`mpremote`](https://docs.micropython.org/en/latest/reference/mpremote.html) and
run the following command in this folder.

``` shell
mpremote connect /dev/serial/by-id/usb-MicroPython_Board_in_FS_mode_* fs cp -r ./**/*.py :
```

If connecting to the bootloader does not work, make sure the *FLASH option register* has the following values set (see [../../miscellaneous/stm32_option_bytes/](../../miscellaneous/stm32_option_bytes/) for how to check these values).

| Bit       | Value |
|-----------|-------|
| nBOOT1    | 1     |
| nSWBOOT0  | 1     |
| BOOT_LOCK | 0     |

Development:

``` shell
mpremote connect /dev/serial/by-id/usb-MicroPython_Board_in_FS_mode_* mount . repl --inject-file main.py
```

## MAC Address

get Wi-Fi mac address:

``` python
import network; wlan=network.WLAN(network.STA_IF); wlan.active(True); mac=wlan.config("mac").hex(); ":".join([mac[i:i+2] for i in range(0,len(mac),2)])
```

MAC addresses of used devices:

- `28:cd:c1:0c:65:22`
- `28:cd:c1:08:1c:ed`
- `28:cd:c1:0c:65:24`
- `28:cd:c1:0f:85:c5`
- `28:cd:c1:0f:85:c7`
- `28:cd:c1:0f:85:c6`

## Notes

- Connect pins 3 and 4 with a jumper to configures the debug add-on to use the *iot-ZHAW* WLAN
