---
title: iwd
outline: [2, 3]
---

# iwd

<div v-pre>

WiFi connection status backed by [IWD](https://iwd.wiki.kernel.org/) (`net.connman.iwd`),
with a dropdown for scanning and switching networks.

Use this instead of the [`network`](/config/modules/network) module on systems where WiFi is
managed by `iwd` rather than NetworkManager. This module is **WiFi only** — IWD does not manage
Ethernet, and (because IWD only handles the wireless link layer) the dropdown does not display an
IP address.

Add it to your layout with `iwd`:

```toml
[[bar.layout]]
monitor = "*"
right = ["iwd"]
```

::: tip Requirements
The `iwd` daemon must be running. When it isn't, the module logs that the service is unavailable
and stays hidden, exactly like the `network` module without NetworkManager.
:::

## General

| Field | Type | Default | Description |
|---|---|---|---|
| `wifi-disabled-icon` | string | `"network-wireless-disabled-symbolic"` | WiFi icon when disabled. |
| `wifi-acquiring-icon` | string | `"network-wireless-acquiring-symbolic"` | WiFi icon when connecting. |
| `wifi-offline-icon` | string | `"network-wireless-offline-symbolic"` | WiFi icon when disconnected. |
| `wifi-connected-icon` | string | `"network-wireless-connected-symbolic"` | WiFi icon when connected but signal strength unavailable. |
| `wifi-signal-icons` | array of string | `[...]` | WiFi signal strength icons from weak to excellent. |
| `border-show` | bool | `false` | Display border around button. |
| `icon-show` | bool | `true` | Display module icon. |
| `label-show` | bool | `true` | Display connection label (the connected SSID). |
| `label-max-length` | u32 | `15` | Max label characters before truncation with ellipsis. Set to 0 to disable. |

::: details More about `wifi-signal-icons`

The signal percentage maps to icons: 0-25% uses icons\[0\], 26-50% uses
icons\[1\], etc.

:::

## Colors

| Field | Type | Default | Description |
|---|---|---|---|
| `border-color` | [`ColorValue`](/config/types#color-value) | `"accent"` | Border color token. |
| `icon-color` | [`ColorValue`](/config/types#color-value) | `"auto"` | Icon foreground color. Auto selects based on variant for contrast. |
| `icon-bg-color` | [`ColorValue`](/config/types#color-value) | `"accent"` | Icon container background color token. |
| `label-color` | [`ColorValue`](/config/types#color-value) | `"accent"` | Label text color token. |
| `button-bg-color` | [`ColorValue`](/config/types#color-value) | `"bg-surface-elevated"` | Button background color token. |

## Click actions

| Field | Type | Default | Description |
|---|---|---|---|
| `left-click` | [`ClickAction`](/config/types#click-action) | `"dropdown:iwd"` | Action on left click. |
| `right-click` | [`ClickAction`](/config/types#click-action) | `""` | Action on right click. |
| `middle-click` | [`ClickAction`](/config/types#click-action) | `""` | Action on middle click. |
| `scroll-up` | [`ClickAction`](/config/types#click-action) | `""` | Action on scroll up. |
| `scroll-down` | [`ClickAction`](/config/types#click-action) | `""` | Action on scroll down. |

## Default configuration

```toml
[modules.iwd]
wifi-disabled-icon = "network-wireless-disabled-symbolic"
wifi-acquiring-icon = "network-wireless-acquiring-symbolic"
wifi-offline-icon = "network-wireless-offline-symbolic"
wifi-connected-icon = "network-wireless-connected-symbolic"
wifi-signal-icons = [
    "network-wireless-signal-weak-symbolic",
    "network-wireless-signal-ok-symbolic",
    "network-wireless-signal-good-symbolic",
    "network-wireless-signal-excellent-symbolic",
]
border-show = false
border-color = "accent"
icon-show = true
icon-color = "auto"
icon-bg-color = "accent"
label-show = true
label-color = "accent"
label-max-length = 15
button-bg-color = "bg-surface-elevated"
left-click = "dropdown:iwd"
right-click = ""
middle-click = ""
scroll-up = ""
scroll-down = ""
```


</div>
