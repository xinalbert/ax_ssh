# Operating-system, distribution and platform icon assets

[中文说明](README.zh.md) · [Offline preview](preview.html) · [Source manifest](manifest.json) · [Search coverage / gaps](search-coverage.json) · [Checks](validation.json)

**327 files** (311 SVG, 16 PNG) cover **177 system/platform entries**. This expansion adds 192 icons and 121 entries; the original 135 SVGs are unchanged. Open `preview.html` locally to compare every file on white and dark backgrounds.

Sources include [Iconify](https://icon-sets.iconify.design/?query=Linux), project websites/brand pages/official repositories, [Papirus](https://github.com/PapirusDevelopmentTeam/papirus-icon-theme), and [Font Logos](https://github.com/lukas-w/font-logos). Symbols are preferred, with native letter emblems retained. Full-name exceptions are explicitly marked. Native SVG/PNG bytes are unchanged; Iconify bodies only receive an SVG wrapper. No logo was cropped or recolored.

## Categories and variants

| Directory | Entries | Contents |
| --- | --- | --- |
| `linux/` | 114 | Linux / GNU systems |
| `macos/` | 1 | macOS |
| `windows/` | 4 | Windows |
| `bsd/` | 7 | BSD systems |
| `appliances/` | 15 | NAS / router / server / media systems |
| `platforms/` | 14 | Cloud / container / virtualization / management platforms |
| `mobile/` | 4 | Mobile systems |
| `other/` | 7 | Other operating systems |
| `desktop/` | 11 | Desktop environments / window managers |

`linux/generic/` contains Tux; Windows subdirectories preserve Windows 8/11 and the legacy flag. `licenses/` contains collection licenses and publisher notices/policies. Entries are not all distributions; the manifest `kind` records their actual category.

Only native upstream variants are saved. A missing light/dark entry means no matching variant was found in the inspected source.

- **selfh.st**: light means white/light ink for dark backgrounds; dark means black/dark ink for light backgrounds.
- **Skill Icons**: light/dark refer to the rounded badge background.
- **Publisher originals**: white/black ink is recorded as light/dark, with `upstream_variant` retained. Volumio dark is its native grey symbol. DietPi keeps its black/inverted names; these are not pure black/white marks. The openKylin white PNG includes a black background. Bazzite LE is another color variant, not dark artwork.
- **Font Logos**: preserved monochrome symbols are original; no artificial theme duplicates were made.

62 entries have saved native light and dark marks/badges; this does not imply every entry also has a separate colorful mark.

## Selection notes and search gaps

Additions include CachyOS, openEuler, Talos, openKylin, UOS, Anolis, OpenCloudOS, BlackArch, Bazzite, SteamOS, Ubuntu Budgie/Studio, Flatcar, DietPi, Bottlerocket, BSD/NAS/router systems, cloud/container platforms and desktop environments. Mageia now has a wordless cauldron; NetBSD prefers a wordless flag; Volumio uses official wordless symbols; the Flatcar official icon files still contain its name.

Preferred full-name exceptions: YunoHost, ZimaOS, IBM AIX, LXLE, MS-DOS, BigLinux, Synology (DSM vendor brand), Amazon Web Services, Oracle Linux, openKylin, OpenCloudOS, UnionTech UOS, DietPi, Flatcar Container Linux, IPFire, LibreELEC, Funtoo Linux. Per-file `text_kind` distinguishes symbol, lettermark and wordmark; the Ubuntu Budgie and original Mageia wordmarks are retained as extra files.

- **Amazon Linux / RancherOS**: no dedicated SVG/PNG mark was confirmed in the inspected sources. Generic AWS and Rancher branding is separately stored under `platforms/`.
- **Synology DSM**: only explicitly labelled vendor branding was selected. Anolis OS and Raspberry Pi OS also document their community/vendor brand proxies.
- **Identity exclusions**: selfhst:endless was not verified as Endless OS; Apache Kylin is not openKylin; token:uos is not UnionTech UOS.
- **Legacy/community artwork**: Puppy remains upstream-hidden; openSUSE ALP/Evergreen/Step remain initiative variants. Funtoo is marked ended by its website. Inclusion of Papirus/Font Logos artwork does not establish current support/release status or official branding.

See [search-coverage.json](search-coverage.json) for individual outcomes. This is the scope of this audit, not an exhaustive list of every operating system.

## Sources and licensing

| Collection | Author/source | Terms | Local notice |
| --- | --- | --- | --- |
| `selfhst` | [selfh.st](https://github.com/selfhst/icons) | CC-BY-4.0 | [LICENSE-selfhst.txt](licenses/LICENSE-selfhst.txt) |
| `thesvg-color` | [thesvg.org](https://github.com/glincker/thesvg) | MIT | [LICENSE-thesvg-color.txt](licenses/LICENSE-thesvg-color.txt) |
| `logos` | [Gil Barbara](https://github.com/gilbarbara/logos) | CC0-1.0 | [LICENSE-logos.txt](licenses/LICENSE-logos.txt) |
| `skill-icons` | [tandpfun](https://github.com/tandpfun/skill-icons) | MIT | [LICENSE-skill-icons.txt](licenses/LICENSE-skill-icons.txt) |
| `papirus` | [Papirus Development Team](https://github.com/PapirusDevelopmentTeam/papirus-icon-theme) | GPL-3.0-only | [LICENSE-papirus.txt](licenses/LICENSE-papirus.txt) |
| `font-logos` | [Lukas W. and contributors](https://github.com/lukas-w/font-logos) | Unlicense | [LICENSE-font-logos.txt](licenses/LICENSE-font-logos.txt) |
| `cncf-artwork` | [CNCF project contributors](https://github.com/cncf/artwork) | Linux Foundation trademark usage guidelines | [NOTICE-cncf-artwork.txt](licenses/NOTICE-cncf-artwork.txt) |
| `official-branding` | Respective project and brand owners | Per-publisher brand terms; no common asset license | [NOTICE-official-branding.txt](licenses/NOTICE-official-branding.txt) |

Collection licenses apply only to the corresponding source files. Publisher brand originals have no common copyright license; where an asset license is not explicit, the manifest records `not-explicitly-stated` rather than assigning MIT/CC0 or the software repository license. CNCF artwork follows Linux Foundation brand terms. Trademarks remain with their owners; preserve the corresponding notices and attribution when distributing assets.

## Checks

SVG XML, viewBoxes or explicit dimensions, hashes, internal references and absence of scripts/text tags/fonts/embedded raster are checked. PNG signatures, dimensions, chunk CRCs and completeness are checked. Every file is rendered at 16px/64px and checked for nonempty output; new asset contact sheets compare light/dark backgrounds. Original publisher/community bytes, Iconify body/viewBox and preservation of the first 135 SVGs are verified. See [validation.json](validation.json). This reviews downloaded artwork, without application screenshots.

## Linux / GNU systems

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| Absolute Linux | distribution | [papirus-absolute.svg](linux/absolute/papirus-absolute.svg) | color | lettermark |
| AcademiX GNU/Linux | distribution | [papirus-academix.svg](linux/academix/papirus-academix.svg) | color | lettermark |
| AlmaLinux | distribution-brand | [selfhst-almalinux.svg](linux/almalinux/selfhst-almalinux.svg) | color, light, dark | symbol |
| Alpine Linux | distribution-brand | [selfhst-alpine-linux.svg](linux/alpine-linux/selfhst-alpine-linux.svg) | color, light, dark | symbol |
| ALT Linux | distribution | [papirus-alt-linux.svg](linux/alt-linux/papirus-alt-linux.svg) | color | symbol |
| Anolis OS (OpenAnolis mark) | distribution | [official-anolis-mark.png](linux/anolis-os/official-anolis-mark.png) | color | symbol |
| antiX | distribution | [papirus-antix.svg](linux/antix/papirus-antix.svg) | color | symbol |
| AOSC OS | distribution | [fontlogos-aosc.svg](linux/aosc/fontlogos-aosc.svg) | original | lettermark |
| Arch Linux | distribution-brand | [selfhst-arch-linux.svg](linux/arch-linux/selfhst-arch-linux.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| ArchBang | distribution | [papirus-archbang.svg](linux/archbang/papirus-archbang.svg) | color | symbol |
| Archcraft | distribution | [fontlogos-archcraft.svg](linux/archcraft/fontlogos-archcraft.svg) | original | symbol |
| ArchLabs | distribution | [papirus-archlabs.svg](linux/archlabs/papirus-archlabs.svg) | color | symbol |
| Archman | distribution | [papirus-archman.svg](linux/archman/papirus-archman.svg) | color | symbol |
| ArcoLinux | distribution | [fontlogos-arcolinux.svg](linux/arcolinux/fontlogos-arcolinux.svg) | original | symbol |
| Artix Linux | distribution-brand | [thesvg-color-artix-linux.svg](linux/artix-linux/thesvg-color-artix-linux.svg) | color | symbol |
| AryaLinux | distribution | [papirus-aryalinux.svg](linux/aryalinux/papirus-aryalinux.svg) | color | lettermark |
| Asahi Linux | distribution-brand | [thesvg-color-asahi-linux.svg](linux/asahi-linux/thesvg-color-asahi-linux.svg) | color | lettermark |
| BackBox | distribution | [papirus-backbox.svg](linux/backbox/papirus-backbox.svg) | color | symbol |
| Bazzite | distribution | [official-bazzite-color.svg](linux/bazzite/official-bazzite-color.svg) | color, light, color (LE) | lettermark |
| BigLinux | distribution | [fontlogos-biglinux.svg](linux/biglinux/fontlogos-biglinux.svg) | original | wordmark |
| BlackArch | distribution | [official-blackarch.png](linux/blackarch/official-blackarch.png) | color | symbol |
| Bodhi Linux | distribution | [papirus-bodhi-linux.svg](linux/bodhi-linux/papirus-bodhi-linux.svg) | color | symbol |
| Bottlerocket | distribution | [official-bottlerocket.png](linux/bottlerocket/official-bottlerocket.png) | color | symbol |
| BunsenLabs | distribution | [papirus-bunsenlabs.svg](linux/bunsenlabs/papirus-bunsenlabs.svg) | color | symbol |
| CachyOS | distribution | [selfhst-cachyos.svg](linux/cachyos/selfhst-cachyos.svg) | color, light, dark | symbol |
| Calculate Linux | distribution | [papirus-calculate-linux.svg](linux/calculate-linux/papirus-calculate-linux.svg) | color | symbol |
| CentOS | distribution-brand | [selfhst-centos.svg](linux/centos/selfhst-centos.svg) | color, light, dark | symbol |
| CloudLinux | distribution-brand | [logos-cloudlinux.svg](linux/cloudlinux/logos-cloudlinux.svg) | color | symbol |
| Condres OS | distribution | [papirus-condres.svg](linux/condres/papirus-condres.svg) | color | symbol |
| CoreOS | distribution | [fontlogos-coreos.svg](linux/coreos/fontlogos-coreos.svg) | original | symbol |
| Debian | distribution-brand | [selfhst-debian.svg](linux/debian/selfhst-debian.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Deepin | distribution-brand | [thesvg-color-deepin.svg](linux/deepin/thesvg-color-deepin.svg) | color | symbol |
| Devuan | distribution-brand | [selfhst-devuan.svg](linux/devuan/selfhst-devuan.svg) | color, light, dark | symbol |
| DietPi | distribution | [official-dietpi.svg](linux/dietpi/official-dietpi.svg) | color, black, inverted | wordmark |
| Edubuntu | distribution-brand | [selfhst-edubuntu.svg](linux/edubuntu/selfhst-edubuntu.svg) | color, light, dark | symbol |
| elementary OS | distribution-brand | [thesvg-color-elementary.svg](linux/elementary-os/thesvg-color-elementary.svg) | color | lettermark |
| EndeavourOS | distribution-brand | [thesvg-color-endeavouros.svg](linux/endeavouros/thesvg-color-endeavouros.svg) | color | symbol |
| Fedora | distribution-brand | [selfhst-fedora.svg](linux/fedora/selfhst-fedora.svg) | color, light, dark | lettermark |
| Flatcar Container Linux | distribution | [cncf-flatcar-color.svg](linux/flatcar/cncf-flatcar-color.svg) | color, light, dark | wordmark |
| Funtoo Linux | distribution | [official-funtoo.png](linux/funtoo/official-funtoo.png) | color | wordmark |
| Garuda Linux | distribution-brand | [thesvg-color-garuda-linux.svg](linux/garuda-linux/thesvg-color-garuda-linux.svg) | color | symbol |
| Gentoo | distribution-brand | [selfhst-gentoo.svg](linux/gentoo/selfhst-gentoo.svg) | color | symbol |
| GNU Guix | distribution | [fontlogos-gnu-guix.svg](linux/gnu-guix/fontlogos-gnu-guix.svg) | original | lettermark |
| Hyperbola GNU/Linux-libre | distribution | [papirus-hyperbola.svg](linux/hyperbola/papirus-hyperbola.svg) | color | symbol |
| k3OS | distribution | [selfhst-rancher-k3os.svg](linux/k3os/selfhst-rancher-k3os.svg) | color, light, dark | symbol |
| Kali Linux | distribution-brand | [selfhst-kali-linux.svg](linux/kali-linux/selfhst-kali-linux.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| KaOS | distribution | [papirus-kaos.svg](linux/kaos/papirus-kaos.svg) | color | lettermark |
| KDE neon | distribution-brand | [thesvg-color-kde-neon.svg](linux/kde-neon/thesvg-color-kde-neon.svg) | color | symbol |
| Knoppix | distribution | [papirus-knoppix.svg](linux/knoppix/papirus-knoppix.svg) | color | symbol |
| Korora | distribution | [papirus-korora.svg](linux/korora/papirus-korora.svg) | color | symbol |
| Kubuntu | distribution-brand | [selfhst-kubuntu.svg](linux/kubuntu/selfhst-kubuntu.svg) | color, light, dark | symbol |
| Linux (Tux) | os-family | [selfhst-linux.svg](linux/generic/selfhst-linux.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Linux Lite | distribution | [papirus-linux-lite.svg](linux/linux-lite/papirus-linux-lite.svg) | color | symbol |
| Linux Mint | distribution-brand | [thesvg-color-linux-mint.svg](linux/linux-mint/thesvg-color-linux-mint.svg) | color, light (badge), dark (badge) | lettermark |
| Lubuntu | distribution-brand | [thesvg-color-lubuntu.svg](linux/lubuntu/thesvg-color-lubuntu.svg) | color | symbol |
| LXLE | distribution | [papirus-lxle.svg](linux/lxle/papirus-lxle.svg) | color | wordmark |
| MadLinux | distribution | [papirus-madlinux.svg](linux/madlinux/papirus-madlinux.svg) | color | symbol |
| Mageia | distribution-brand | [fontlogos-mageia.svg](linux/mageia/fontlogos-mageia.svg) | color, original | symbol |
| Mandriva | distribution | [fontlogos-mandriva.svg](linux/mandriva/fontlogos-mandriva.svg) | original | symbol |
| Manjaro | distribution-brand | [thesvg-color-manjaro.svg](linux/manjaro/thesvg-color-manjaro.svg) | color | lettermark |
| MX Linux | distribution-brand | [thesvg-color-mx-linux.svg](linux/mx-linux/thesvg-color-mx-linux.svg) | original | lettermark |
| Netrunner | distribution | [papirus-netrunner.svg](linux/netrunner/papirus-netrunner.svg) | color | symbol |
| NixOS | distribution-brand | [selfhst-nixos.svg](linux/nixos/selfhst-nixos.svg) | color, light, dark | symbol |
| Nobara Linux | distribution-brand | [thesvg-color-nobara-linux.svg](linux/nobara-linux/thesvg-color-nobara-linux.svg) | original | lettermark |
| Omarchy | distribution | [thesvg-color-omarchy.svg](linux/omarchy/thesvg-color-omarchy.svg) | color | lettermark |
| OpenCloudOS | distribution | [official-opencloudos.png](linux/opencloudos/official-opencloudos.png) | color | wordmark |
| openEuler | distribution | [selfhst-openeuler.svg](linux/openeuler/selfhst-openeuler.svg) | color, light, dark | symbol |
| openKylin | distribution | [official-openkylin-color.svg](linux/openkylin/official-openkylin-color.svg) | color, light, dark | wordmark |
| OpenMandriva | distribution | [papirus-openmandriva.svg](linux/openmandriva/papirus-openmandriva.svg) | color | symbol |
| openSUSE | distribution-brand | [selfhst-opensuse.svg](linux/opensuse/selfhst-opensuse.svg) | color, light, dark | symbol |
| openSUSE ALP | distribution-brand | [selfhst-opensuse-alp.svg](linux/opensuse-alp/selfhst-opensuse-alp.svg) | color, light, dark | symbol |
| openSUSE Evergreen | distribution-brand | [selfhst-opensuse-evergreen.svg](linux/opensuse-evergreen/selfhst-opensuse-evergreen.svg) | color, light, dark | symbol |
| openSUSE Leap | distribution-brand | [selfhst-opensuse-leap.svg](linux/opensuse-leap/selfhst-opensuse-leap.svg) | color, light, dark | symbol |
| openSUSE Step | distribution-brand | [selfhst-opensuse-step.svg](linux/opensuse-step/selfhst-opensuse-step.svg) | color, light, dark | symbol |
| openSUSE Tumbleweed | distribution-brand | [selfhst-opensuse-tumbleweed.svg](linux/opensuse-tumbleweed/selfhst-opensuse-tumbleweed.svg) | original, light, dark | symbol |
| Oracle Linux | distribution | [official-oracle-linux.png](linux/oracle-linux/official-oracle-linux.png) | color | wordmark |
| Parabola GNU/Linux-libre | distribution | [fontlogos-parabola.svg](linux/parabola/fontlogos-parabola.svg) | original | lettermark |
| Parrot OS | distribution-brand | [thesvg-color-parrot-security.svg](linux/parrot-os/thesvg-color-parrot-security.svg) | color | symbol |
| PCLinuxOS | distribution | [papirus-pclinuxos.svg](linux/pclinuxos/papirus-pclinuxos.svg) | color | symbol |
| Peppermint OS | distribution-brand | [selfhst-peppermint.svg](linux/peppermint/selfhst-peppermint.svg) | color, light, dark | symbol |
| Pisi Linux | distribution | [papirus-pisilinux.svg](linux/pisilinux/papirus-pisilinux.svg) | color | symbol |
| Pop!_OS | distribution-brand | [thesvg-color-pop-os.svg](linux/pop-os/thesvg-color-pop-os.svg) | color | lettermark |
| Puppy Linux | distribution-brand | [logos-puppy-linux.svg](linux/puppy-linux/logos-puppy-linux.svg) | color | symbol |
| PureOS | distribution | [papirus-pureos.svg](linux/pureos/papirus-pureos.svg) | color | symbol |
| Qubes OS | distribution-brand | [thesvg-color-qubes-os.svg](linux/qubes-os/thesvg-color-qubes-os.svg) | color | lettermark |
| Raspberry Pi / Raspberry Pi OS | distribution-brand | [selfhst-raspberry-pi.svg](linux/raspberry-pi-os/selfhst-raspberry-pi.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Red Hat / RHEL | distribution-brand | [selfhst-red-hat.svg](linux/red-hat/selfhst-red-hat.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Red Hat Enterprise Linux | distribution | [selfhst-rhel.svg](linux/rhel/selfhst-rhel.svg) | color, light, dark | symbol |
| Rocky Linux | distribution-brand | [selfhst-rocky-linux.svg](linux/rocky-linux/selfhst-rocky-linux.svg) | color, light, dark | symbol |
| ROSA Linux | distribution | [papirus-rosa.svg](linux/rosa/papirus-rosa.svg) | color | symbol |
| Sabayon | distribution | [fontlogos-sabayon.svg](linux/sabayon/fontlogos-sabayon.svg) | original | symbol |
| SalentOS | distribution | [papirus-salentos.svg](linux/salentos/papirus-salentos.svg) | color | symbol |
| Simply Linux | distribution | [papirus-simply.svg](linux/simply/papirus-simply.svg) | color | lettermark |
| Slackware | distribution-brand | [thesvg-color-slackware.svg](linux/slackware/thesvg-color-slackware.svg) | original | lettermark |
| Solus | distribution-brand | [thesvg-color-solus.svg](linux/solus/thesvg-color-solus.svg) | color | symbol |
| SteamOS | distribution | [papirus-steamos.svg](linux/steamos/papirus-steamos.svg) | color | symbol |
| SUSE | distribution-brand | [thesvg-color-suse.svg](linux/suse/thesvg-color-suse.svg) | color | symbol |
| Tails | distribution-brand | [thesvg-color-tails.svg](linux/tails/thesvg-color-tails.svg) | color | lettermark |
| Talos Linux | distribution | [selfhst-sidero-talos.svg](linux/talos-linux/selfhst-sidero-talos.svg) | color, light, dark | symbol |
| Trisquel GNU/Linux | distribution | [fontlogos-trisquel.svg](linux/trisquel/fontlogos-trisquel.svg) | original | symbol |
| TurnKey Linux | distribution-brand | [selfhst-turnkey-linux.svg](linux/turnkey-linux/selfhst-turnkey-linux.svg) | color, light, dark | lettermark |
| TUXEDO OS | distribution | [papirus-tuxedo-os.svg](linux/tuxedo-os/papirus-tuxedo-os.svg) | color | lettermark |
| Ubuntu | distribution-brand | [selfhst-ubuntu.svg](linux/ubuntu/selfhst-ubuntu.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Ubuntu Budgie | distribution | [papirus-ubuntu-budgie.svg](linux/ubuntu-budgie/papirus-ubuntu-budgie.svg) | color, light | symbol |
| Ubuntu MATE | distribution-brand | [thesvg-color-ubuntu-mate.svg](linux/ubuntu-mate/thesvg-color-ubuntu-mate.svg) | color | symbol |
| Ubuntu Studio | distribution | [papirus-ubuntu-studio.svg](linux/ubuntu-studio/papirus-ubuntu-studio.svg) | color, light | symbol |
| Ultimate Edition | distribution | [papirus-ultimate-edition.svg](linux/ultimate-edition/papirus-ultimate-edition.svg) | color | lettermark |
| UnionTech UOS | distribution | [official-uos.png](linux/uos/official-uos.png) | color | wordmark |
| Vanilla OS | distribution | [papirus-vanilla-os.svg](linux/vanilla-os/papirus-vanilla-os.svg) | color | symbol |
| Void Linux | distribution-brand | [thesvg-color-void-linux.svg](linux/void-linux/thesvg-color-void-linux.svg) | color | symbol |
| Voyager Live | distribution | [papirus-voyager.svg](linux/voyager/papirus-voyager.svg) | color | lettermark |
| XeroLinux | distribution | [fontlogos-xerolinux.svg](linux/xerolinux/fontlogos-xerolinux.svg) | original | lettermark |
| Xubuntu | distribution-brand | [selfhst-xubuntu.svg](linux/xubuntu/selfhst-xubuntu.svg) | color, light, dark | symbol |
| Zorin OS | distribution-brand | [selfhst-zorin-os.svg](linux/zorin-os/selfhst-zorin-os.svg) | color, light, dark | lettermark |

## macOS

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| macOS / Apple | operating-system | [selfhst-apple.svg](macos/selfhst-apple.svg) | original, light, dark, light (badge), dark (badge), color | symbol |

## Windows

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| Windows | operating-system | [selfhst-microsoft-windows.svg](windows/selfhst-microsoft-windows.svg) | color, light, dark, light (badge), dark (badge) | symbol |
| Windows (legacy flag) | operating-system | [selfhst-windows-retro.svg](windows/retro/selfhst-windows-retro.svg) | color, light, dark | symbol |
| Windows 11 | operating-system | [thesvg-color-windows11.svg](windows/windows-11/thesvg-color-windows11.svg) | color | symbol |
| Windows 8 | operating-system | [thesvg-color-windows8.svg](windows/windows-8/thesvg-color-windows8.svg) | color | symbol |

## BSD systems

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| DragonFly BSD | operating-system | [papirus-dragonflybsd.svg](bsd/dragonflybsd/papirus-dragonflybsd.svg) | color | symbol |
| FreeBSD | operating-system | [selfhst-freebsd.svg](bsd/freebsd/selfhst-freebsd.svg) | color, light, dark | symbol |
| GhostBSD | operating-system | [papirus-ghostbsd.svg](bsd/ghostbsd/papirus-ghostbsd.svg) | color | lettermark |
| MidnightBSD | operating-system | [papirus-midnightbsd.svg](bsd/midnightbsd/papirus-midnightbsd.svg) | color | symbol |
| NetBSD | operating-system | [papirus-netbsd.svg](bsd/netbsd/papirus-netbsd.svg) | color | symbol |
| OpenBSD | operating-system | [thesvg-color-openbsd.svg](bsd/openbsd/thesvg-color-openbsd.svg) | color | symbol |
| TrueOS | operating-system | [papirus-trueos.svg](bsd/trueos/papirus-trueos.svg) | color | symbol |

## NAS / router / server / media systems

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| IPFire | router-os | [official-ipfire.svg](appliances/ipfire/official-ipfire.svg) | color, light, dark | wordmark |
| LibreELEC | media-os | [official-libreelec.png](appliances/libreelec/official-libreelec.png) | color | wordmark |
| OpenMediaVault | nas-os | [selfhst-openmediavault.svg](appliances/openmediavault/selfhst-openmediavault.svg) | color, light, dark | symbol |
| OpenWrt | router-os | [selfhst-openwrt.svg](appliances/openwrt/selfhst-openwrt.svg) | color, light, dark | symbol |
| OPNsense | router-os | [selfhst-opnsense.svg](appliances/opnsense/selfhst-opnsense.svg) | color, light, dark | symbol |
| OSMC | media-os | [thesvg-color-osmc.svg](appliances/osmc/thesvg-color-osmc.svg) | color | lettermark |
| pfSense | router-os | [selfhst-pfsense.svg](appliances/pfsense/selfhst-pfsense.svg) | original, light, dark | lettermark |
| TrueNAS CORE | nas-os | [selfhst-truenas-core.svg](appliances/truenas-core/selfhst-truenas-core.svg) | color, light, dark | symbol |
| TrueNAS SCALE | nas-os | [selfhst-truenas-scale.svg](appliances/truenas-scale/selfhst-truenas-scale.svg) | color, light, dark | symbol |
| umbrelOS | server-os | [selfhst-umbrelos.svg](appliances/umbrelos/selfhst-umbrelos.svg) | color, light, dark | lettermark |
| Unraid | nas-os | [selfhst-unraid.svg](appliances/unraid/selfhst-unraid.svg) | color, light, dark | symbol |
| Volumio | media-os | [official-volumio-pack-volumio-icon.png](appliances/volumio/official-volumio-pack-volumio-icon.png) | color, light, dark | symbol |
| VyOS | router-os | [official-vyos.svg](appliances/vyos/official-vyos.svg) | color | lettermark |
| YunoHost | server-os | [selfhst-yunohost.svg](appliances/yunohost/selfhst-yunohost.svg) | original, light, dark | wordmark |
| ZimaOS | nas-os | [selfhst-zimaos.svg](appliances/zimaos/selfhst-zimaos.svg) | original, light, dark | wordmark |

## Cloud / container / virtualization / management platforms

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| Amazon Web Services | cloud-platform | [selfhst-amazon-web-services.svg](platforms/aws/selfhst-amazon-web-services.svg) | color, light, dark | wordmark |
| CasaOS | server-management | [selfhst-casaos.svg](platforms/casaos/selfhst-casaos.svg) | color, light, dark | symbol |
| Docker | container-platform | [selfhst-docker.svg](platforms/docker/selfhst-docker.svg) | color, light, dark | symbol |
| HomeDock OS | server-management | [selfhst-homedock-os.svg](platforms/homedock-os/selfhst-homedock-os.svg) | original, light, dark | symbol |
| Kubernetes | container-platform | [selfhst-kubernetes.svg](platforms/kubernetes/selfhst-kubernetes.svg) | color, light, dark | symbol |
| Livinity | server-management | [selfhst-livinity.svg](platforms/livinity/selfhst-livinity.svg) | original, light, dark | symbol |
| OpenStack | cloud-platform | [thesvg-color-openstack.svg](platforms/openstack/thesvg-color-openstack.svg) | color | symbol |
| Proxmox VE | virtualization-platform | [selfhst-proxmox.svg](platforms/proxmox/selfhst-proxmox.svg) | color, light, dark | lettermark |
| QEMU | virtualization-platform | [thesvg-color-qemu.svg](platforms/qemu/thesvg-color-qemu.svg) | color | symbol |
| Rancher | container-management | [selfhst-rancher.svg](platforms/rancher/selfhst-rancher.svg) | color, light, dark | symbol |
| RetroPie | gaming-platform | [thesvg-color-retropie.svg](platforms/retropie/thesvg-color-retropie.svg) | color | symbol |
| Synology (DSM vendor brand) | vendor-brand | [selfhst-synology.svg](platforms/synology/selfhst-synology.svg) | original, light, dark | wordmark |
| VMware ESX | virtualization-platform | [selfhst-vmware-esx.svg](platforms/vmware-esx/selfhst-vmware-esx.svg) | color, light, dark | symbol |
| Xen Orchestra | virtualization-management | [selfhst-xen-orchestra.svg](platforms/xen-orchestra/selfhst-xen-orchestra.svg) | color | symbol |

## Mobile systems

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| Android | operating-system | [selfhst-android-robot.svg](mobile/android/selfhst-android-robot.svg) | color, light, dark | symbol |
| GrapheneOS | operating-system | [selfhst-grapheneos.svg](mobile/grapheneos/selfhst-grapheneos.svg) | color, light, dark | symbol |
| postmarketOS | operating-system | [fontlogos-postmarketos.svg](mobile/postmarketos/fontlogos-postmarketos.svg) | original | symbol |
| Sailfish OS | operating-system | [thesvg-color-sailfish-os.svg](mobile/sailfish-os/thesvg-color-sailfish-os.svg) | color | symbol |

## Other operating systems

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| FreeDOS | operating-system | [papirus-freedos.svg](other/freedos/papirus-freedos.svg) | color | symbol |
| Haiku | operating-system | [logos-haiku-icon.svg](other/haiku/logos-haiku-icon.svg) | color | lettermark |
| IBM AIX | operating-system | [papirus-aix.svg](other/aix/papirus-aix.svg) | color | wordmark |
| illumos | operating-system | [fontlogos-illumos.svg](other/illumos/fontlogos-illumos.svg) | original | lettermark |
| MS-DOS | operating-system | [papirus-ms-dos.svg](other/ms-dos/papirus-ms-dos.svg) | color | wordmark |
| ReactOS | operating-system | [thesvg-color-reactos.svg](other/reactos/thesvg-color-reactos.svg) | color | symbol |
| SmartOS | operating-system | [papirus-smartos.svg](other/smartos/papirus-smartos.svg) | color | lettermark |

## Desktop environments / window managers

| Name | Kind | Preferred icon | Saved variants | Text kind |
| --- | --- | --- | --- | --- |
| Budgie | desktop-environment | [fontlogos-budgie.svg](desktop/budgie/fontlogos-budgie.svg) | original | symbol |
| Cinnamon | desktop-environment | [thesvg-color-cinnamon.svg](desktop/cinnamon/thesvg-color-cinnamon.svg) | color | symbol |
| GNOME | desktop-environment | [thesvg-color-gnome.svg](desktop/gnome/thesvg-color-gnome.svg) | color | symbol |
| Hyprland | window-manager | [thesvg-color-hyprland.svg](desktop/hyprland/thesvg-color-hyprland.svg) | color | symbol |
| i3 | window-manager | [thesvg-color-i3.svg](desktop/i3/thesvg-color-i3.svg) | color | lettermark |
| KDE | desktop-environment | [thesvg-color-kde.svg](desktop/kde/thesvg-color-kde.svg) | color | lettermark |
| LXDE | desktop-environment | [fontlogos-lxde.svg](desktop/lxde/fontlogos-lxde.svg) | original | symbol |
| LXQt | desktop-environment | [fontlogos-lxqt.svg](desktop/lxqt/fontlogos-lxqt.svg) | original | lettermark |
| MATE | desktop-environment | [fontlogos-mate.svg](desktop/mate/fontlogos-mate.svg) | original | symbol |
| Sway | window-manager | [thesvg-color-sway.svg](desktop/sway/thesvg-color-sway.svg) | color | symbol |
| Xfce | desktop-environment | [thesvg-color-xfce.svg](desktop/xfce/thesvg-color-xfce.svg) | color | symbol |
