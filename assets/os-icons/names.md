# System icon name index

[中文](names.zh.md) · [Catalog](catalog.json) · [Assets and licenses](README.md)

Canonical IDs use lowercase letters, digits and hyphens; display names retain branding.
Aliases match words without case or punctuation; specific distributions/editions precede families.
`linux` is the Tux fallback; unknown non-Linux systems use generic server/terminal glyphs.
Local Linux uses its os-release ID and does not inherit an ID_LIKE parent icon.

After editing catalog.json, generate and check from the repository root:

```bash
python3 scripts/generate_system_icons.py
python3 scripts/generate_system_icons.py --check
```

| Canonical ID | Display name | Family | Recognition aliases | Icon path |
| --- | --- | --- | --- | --- |
| `absolute` | Absolute Linux | linux | `absolute linux`, `absolute` | [linux/absolute/papirus-absolute.svg](linux/absolute/papirus-absolute.svg) |
| `academix` | AcademiX GNU/Linux | linux | `academix gnu linux`, `academix` | [linux/academix/papirus-academix.svg](linux/academix/papirus-academix.svg) |
| `almalinux` | AlmaLinux | linux | `alma linux`, `almalinux` | [linux/almalinux/selfhst-almalinux.svg](linux/almalinux/selfhst-almalinux.svg) |
| `alpine-linux` | Alpine Linux | linux | `alpine linux`, `alpine` | [linux/alpine-linux/selfhst-alpine-linux.svg](linux/alpine-linux/selfhst-alpine-linux.svg) |
| `alt-linux` | ALT Linux | linux | `alt linux`, `altlinux` | [linux/alt-linux/papirus-alt-linux.svg](linux/alt-linux/papirus-alt-linux.svg) |
| `anolis-os` | Anolis OS | linux | `openanolis`, `anolis os`, `anolis` | [linux/anolis-os/official-anolis-mark.png](linux/anolis-os/official-anolis-mark.png) |
| `antix` | antiX | linux | `antix` | [linux/antix/papirus-antix.svg](linux/antix/papirus-antix.svg) |
| `aosc` | AOSC OS | linux | `aosc os`, `aosc` | [linux/aosc/fontlogos-aosc.svg](linux/aosc/fontlogos-aosc.svg) |
| `arch-linux` | Arch Linux | linux | `arch linux`, `arch` | [linux/arch-linux/selfhst-arch-linux.svg](linux/arch-linux/selfhst-arch-linux.svg) |
| `archbang` | ArchBang | linux | `archbang` | [linux/archbang/papirus-archbang.svg](linux/archbang/papirus-archbang.svg) |
| `archcraft` | Archcraft | linux | `archcraft` | [linux/archcraft/fontlogos-archcraft.svg](linux/archcraft/fontlogos-archcraft.svg) |
| `archlabs` | ArchLabs | linux | `archlabs` | [linux/archlabs/papirus-archlabs.svg](linux/archlabs/papirus-archlabs.svg) |
| `archman` | Archman | linux | `archman` | [linux/archman/papirus-archman.svg](linux/archman/papirus-archman.svg) |
| `arcolinux` | ArcoLinux | linux | `arco linux`, `arcolinux` | [linux/arcolinux/fontlogos-arcolinux.svg](linux/arcolinux/fontlogos-arcolinux.svg) |
| `artix-linux` | Artix Linux | linux | `artix linux`, `artix` | [linux/artix-linux/thesvg-color-artix-linux.svg](linux/artix-linux/thesvg-color-artix-linux.svg) |
| `aryalinux` | AryaLinux | linux | `aryalinux` | [linux/aryalinux/papirus-aryalinux.svg](linux/aryalinux/papirus-aryalinux.svg) |
| `asahi-linux` | Asahi Linux | linux | `asahi linux`, `asahi` | [linux/asahi-linux/thesvg-color-asahi-linux.svg](linux/asahi-linux/thesvg-color-asahi-linux.svg) |
| `backbox` | BackBox | linux | `backbox` | [linux/backbox/papirus-backbox.svg](linux/backbox/papirus-backbox.svg) |
| `bazzite` | Bazzite | linux | `bazzite` | [linux/bazzite/official-bazzite-color.svg](linux/bazzite/official-bazzite-color.svg) |
| `biglinux` | BigLinux | linux | `biglinux` | [linux/biglinux/fontlogos-biglinux.svg](linux/biglinux/fontlogos-biglinux.svg) |
| `blackarch` | BlackArch | linux | `blackarch` | [linux/blackarch/official-blackarch.png](linux/blackarch/official-blackarch.png) |
| `bodhi-linux` | Bodhi Linux | linux | `bodhi linux`, `bodhi` | [linux/bodhi-linux/papirus-bodhi-linux.svg](linux/bodhi-linux/papirus-bodhi-linux.svg) |
| `bottlerocket` | Bottlerocket | linux | `bottlerocket` | [linux/bottlerocket/official-bottlerocket.png](linux/bottlerocket/official-bottlerocket.png) |
| `bunsenlabs` | BunsenLabs | linux | `bunsenlabs` | [linux/bunsenlabs/papirus-bunsenlabs.svg](linux/bunsenlabs/papirus-bunsenlabs.svg) |
| `cachyos` | CachyOS | linux | `cachyos` | [linux/cachyos/selfhst-cachyos.svg](linux/cachyos/selfhst-cachyos.svg) |
| `calculate-linux` | Calculate Linux | linux | `calculate linux`, `calculate` | [linux/calculate-linux/papirus-calculate-linux.svg](linux/calculate-linux/papirus-calculate-linux.svg) |
| `centos` | CentOS | linux | `centos` | [linux/centos/selfhst-centos.svg](linux/centos/selfhst-centos.svg) |
| `cloudlinux` | CloudLinux | linux | `cloudlinux` | [linux/cloudlinux/logos-cloudlinux.svg](linux/cloudlinux/logos-cloudlinux.svg) |
| `condres` | Condres OS | linux | `condres os`, `condres` | [linux/condres/papirus-condres.svg](linux/condres/papirus-condres.svg) |
| `coreos` | CoreOS | linux | `coreos` | [linux/coreos/fontlogos-coreos.svg](linux/coreos/fontlogos-coreos.svg) |
| `debian` | Debian | linux | `debian` | [linux/debian/selfhst-debian.svg](linux/debian/selfhst-debian.svg) |
| `deepin` | Deepin | linux | `deepin` | [linux/deepin/thesvg-color-deepin.svg](linux/deepin/thesvg-color-deepin.svg) |
| `devuan` | Devuan | linux | `devuan` | [linux/devuan/selfhst-devuan.svg](linux/devuan/selfhst-devuan.svg) |
| `dietpi` | DietPi | linux | `dietpi` | [linux/dietpi/official-dietpi.svg](linux/dietpi/official-dietpi.svg) |
| `edubuntu` | Edubuntu | linux | `edubuntu` | [linux/edubuntu/selfhst-edubuntu.svg](linux/edubuntu/selfhst-edubuntu.svg) |
| `elementary-os` | elementary OS | linux | `elementary os`, `elementary` | [linux/elementary-os/thesvg-color-elementary.svg](linux/elementary-os/thesvg-color-elementary.svg) |
| `endeavouros` | EndeavourOS | linux | `endeavouros` | [linux/endeavouros/thesvg-color-endeavouros.svg](linux/endeavouros/thesvg-color-endeavouros.svg) |
| `fedora` | Fedora | linux | `fedora` | [linux/fedora/selfhst-fedora.svg](linux/fedora/selfhst-fedora.svg) |
| `flatcar` | Flatcar Container Linux | linux | `flatcar container linux`, `flatcar` | [linux/flatcar/cncf-flatcar-color.svg](linux/flatcar/cncf-flatcar-color.svg) |
| `funtoo` | Funtoo Linux | linux | `funtoo linux`, `funtoo` | [linux/funtoo/official-funtoo.png](linux/funtoo/official-funtoo.png) |
| `garuda-linux` | Garuda Linux | linux | `garuda linux`, `garuda` | [linux/garuda-linux/thesvg-color-garuda-linux.svg](linux/garuda-linux/thesvg-color-garuda-linux.svg) |
| `gentoo` | Gentoo | linux | `gentoo` | [linux/gentoo/selfhst-gentoo.svg](linux/gentoo/selfhst-gentoo.svg) |
| `gnu-guix` | GNU Guix | linux | `guix system`, `gnu guix`, `guix` | [linux/gnu-guix/fontlogos-gnu-guix.svg](linux/gnu-guix/fontlogos-gnu-guix.svg) |
| `hyperbola` | Hyperbola GNU/Linux-libre | linux | `hyperbola gnu linux libre`, `hyperbola` | [linux/hyperbola/papirus-hyperbola.svg](linux/hyperbola/papirus-hyperbola.svg) |
| `k3os` | k3OS | linux | `k3os` | [linux/k3os/selfhst-rancher-k3os.svg](linux/k3os/selfhst-rancher-k3os.svg) |
| `kali-linux` | Kali Linux | linux | `kali linux`, `kali` | [linux/kali-linux/selfhst-kali-linux.svg](linux/kali-linux/selfhst-kali-linux.svg) |
| `kaos` | KaOS | linux | `kaos` | [linux/kaos/papirus-kaos.svg](linux/kaos/papirus-kaos.svg) |
| `kde-neon` | KDE neon | linux | `kde neon`, `neon` | [linux/kde-neon/thesvg-color-kde-neon.svg](linux/kde-neon/thesvg-color-kde-neon.svg) |
| `knoppix` | Knoppix | linux | `knoppix` | [linux/knoppix/papirus-knoppix.svg](linux/knoppix/papirus-knoppix.svg) |
| `korora` | Korora | linux | `korora` | [linux/korora/papirus-korora.svg](linux/korora/papirus-korora.svg) |
| `kubuntu` | Kubuntu | linux | `kubuntu` | [linux/kubuntu/selfhst-kubuntu.svg](linux/kubuntu/selfhst-kubuntu.svg) |
| `linux` | Linux | linux | `linux` | [linux/generic/selfhst-linux.svg](linux/generic/selfhst-linux.svg) |
| `linux-lite` | Linux Lite | linux | `linux lite` | [linux/linux-lite/papirus-linux-lite.svg](linux/linux-lite/papirus-linux-lite.svg) |
| `linux-mint` | Linux Mint | linux | `linux mint`, `linuxmint` | [linux/linux-mint/thesvg-color-linux-mint.svg](linux/linux-mint/thesvg-color-linux-mint.svg) |
| `lubuntu` | Lubuntu | linux | `lubuntu` | [linux/lubuntu/thesvg-color-lubuntu.svg](linux/lubuntu/thesvg-color-lubuntu.svg) |
| `lxle` | LXLE | linux | `lxle` | [linux/lxle/papirus-lxle.svg](linux/lxle/papirus-lxle.svg) |
| `madlinux` | MadLinux | linux | `madlinux` | [linux/madlinux/papirus-madlinux.svg](linux/madlinux/papirus-madlinux.svg) |
| `mageia` | Mageia | linux | `mageia` | [linux/mageia/fontlogos-mageia.svg](linux/mageia/fontlogos-mageia.svg) |
| `mandriva` | Mandriva | linux | `mandriva` | [linux/mandriva/fontlogos-mandriva.svg](linux/mandriva/fontlogos-mandriva.svg) |
| `manjaro` | Manjaro | linux | `manjaro` | [linux/manjaro/thesvg-color-manjaro.svg](linux/manjaro/thesvg-color-manjaro.svg) |
| `mx-linux` | MX Linux | linux | `mx linux`, `mx` | [linux/mx-linux/thesvg-color-mx-linux.svg](linux/mx-linux/thesvg-color-mx-linux.svg) |
| `netrunner` | Netrunner | linux | `netrunner` | [linux/netrunner/papirus-netrunner.svg](linux/netrunner/papirus-netrunner.svg) |
| `nixos` | NixOS | linux | `nixos` | [linux/nixos/selfhst-nixos.svg](linux/nixos/selfhst-nixos.svg) |
| `nobara-linux` | Nobara Linux | linux | `nobara linux`, `nobara` | [linux/nobara-linux/thesvg-color-nobara-linux.svg](linux/nobara-linux/thesvg-color-nobara-linux.svg) |
| `omarchy` | Omarchy | linux | `omarchy` | [linux/omarchy/thesvg-color-omarchy.svg](linux/omarchy/thesvg-color-omarchy.svg) |
| `opencloudos` | OpenCloudOS | linux | `opencloudos` | [linux/opencloudos/official-opencloudos.png](linux/opencloudos/official-opencloudos.png) |
| `openeuler` | openEuler | linux | `openeuler` | [linux/openeuler/selfhst-openeuler.svg](linux/openeuler/selfhst-openeuler.svg) |
| `openkylin` | openKylin | linux | `openkylin` | [linux/openkylin/official-openkylin-color.svg](linux/openkylin/official-openkylin-color.svg) |
| `openmandriva` | OpenMandriva | linux | `openmandriva` | [linux/openmandriva/papirus-openmandriva.svg](linux/openmandriva/papirus-openmandriva.svg) |
| `opensuse` | openSUSE | linux | `opensuse` | [linux/opensuse/selfhst-opensuse.svg](linux/opensuse/selfhst-opensuse.svg) |
| `opensuse-alp` | openSUSE ALP | linux | `opensuse alp` | [linux/opensuse-alp/selfhst-opensuse-alp.svg](linux/opensuse-alp/selfhst-opensuse-alp.svg) |
| `opensuse-evergreen` | openSUSE Evergreen | linux | `opensuse evergreen` | [linux/opensuse-evergreen/selfhst-opensuse-evergreen.svg](linux/opensuse-evergreen/selfhst-opensuse-evergreen.svg) |
| `opensuse-leap` | openSUSE Leap | linux | `opensuse leap` | [linux/opensuse-leap/selfhst-opensuse-leap.svg](linux/opensuse-leap/selfhst-opensuse-leap.svg) |
| `opensuse-step` | openSUSE Step | linux | `opensuse step` | [linux/opensuse-step/selfhst-opensuse-step.svg](linux/opensuse-step/selfhst-opensuse-step.svg) |
| `opensuse-tumbleweed` | openSUSE Tumbleweed | linux | `opensuse tumbleweed` | [linux/opensuse-tumbleweed/selfhst-opensuse-tumbleweed.svg](linux/opensuse-tumbleweed/selfhst-opensuse-tumbleweed.svg) |
| `oracle-linux` | Oracle Linux | linux | `oracle linux`, `oraclelinux`, `ol` | [linux/oracle-linux/official-oracle-linux.png](linux/oracle-linux/official-oracle-linux.png) |
| `parabola` | Parabola GNU/Linux-libre | linux | `parabola gnu linux libre`, `parabola` | [linux/parabola/fontlogos-parabola.svg](linux/parabola/fontlogos-parabola.svg) |
| `parrot-os` | Parrot OS | linux | `parrot security`, `parrot os`, `parrot` | [linux/parrot-os/thesvg-color-parrot-security.svg](linux/parrot-os/thesvg-color-parrot-security.svg) |
| `pclinuxos` | PCLinuxOS | linux | `pclinuxos` | [linux/pclinuxos/papirus-pclinuxos.svg](linux/pclinuxos/papirus-pclinuxos.svg) |
| `peppermint` | Peppermint OS | linux | `peppermint os`, `peppermint` | [linux/peppermint/selfhst-peppermint.svg](linux/peppermint/selfhst-peppermint.svg) |
| `pisilinux` | Pisi Linux | linux | `pisi linux`, `pisilinux` | [linux/pisilinux/papirus-pisilinux.svg](linux/pisilinux/papirus-pisilinux.svg) |
| `pop-os` | Pop!_OS | linux | `pop os`, `pop` | [linux/pop-os/thesvg-color-pop-os.svg](linux/pop-os/thesvg-color-pop-os.svg) |
| `puppy-linux` | Puppy Linux | linux | `puppy linux`, `puppy` | [linux/puppy-linux/logos-puppy-linux.svg](linux/puppy-linux/logos-puppy-linux.svg) |
| `pureos` | PureOS | linux | `pureos` | [linux/pureos/papirus-pureos.svg](linux/pureos/papirus-pureos.svg) |
| `qubes-os` | Qubes OS | linux | `qubes os` | [linux/qubes-os/thesvg-color-qubes-os.svg](linux/qubes-os/thesvg-color-qubes-os.svg) |
| `raspberry-pi-os` | Raspberry Pi OS | linux | `raspberry pi os`, `raspberry pi`, `raspbian` | [linux/raspberry-pi-os/selfhst-raspberry-pi.svg](linux/raspberry-pi-os/selfhst-raspberry-pi.svg) |
| `red-hat` | Red Hat | linux | `red hat` | [linux/red-hat/selfhst-red-hat.svg](linux/red-hat/selfhst-red-hat.svg) |
| `rhel` | Red Hat Enterprise Linux | linux | `red hat enterprise linux`, `rhel` | [linux/rhel/selfhst-rhel.svg](linux/rhel/selfhst-rhel.svg) |
| `rocky-linux` | Rocky Linux | linux | `rocky linux`, `rocky` | [linux/rocky-linux/selfhst-rocky-linux.svg](linux/rocky-linux/selfhst-rocky-linux.svg) |
| `rosa` | ROSA Linux | linux | `rosa linux`, `rosa` | [linux/rosa/papirus-rosa.svg](linux/rosa/papirus-rosa.svg) |
| `sabayon` | Sabayon | linux | `sabayon` | [linux/sabayon/fontlogos-sabayon.svg](linux/sabayon/fontlogos-sabayon.svg) |
| `salentos` | SalentOS | linux | `salentos` | [linux/salentos/papirus-salentos.svg](linux/salentos/papirus-salentos.svg) |
| `simply` | Simply Linux | linux | `simply linux`, `simply` | [linux/simply/papirus-simply.svg](linux/simply/papirus-simply.svg) |
| `slackware` | Slackware | linux | `slackware` | [linux/slackware/thesvg-color-slackware.svg](linux/slackware/thesvg-color-slackware.svg) |
| `solus` | Solus | linux | `solus` | [linux/solus/thesvg-color-solus.svg](linux/solus/thesvg-color-solus.svg) |
| `steamos` | SteamOS | linux | `steamos` | [linux/steamos/papirus-steamos.svg](linux/steamos/papirus-steamos.svg) |
| `suse` | SUSE | linux | `sled`, `sles`, `suse` | [linux/suse/thesvg-color-suse.svg](linux/suse/thesvg-color-suse.svg) |
| `tails` | Tails | linux | `tails` | [linux/tails/thesvg-color-tails.svg](linux/tails/thesvg-color-tails.svg) |
| `talos-linux` | Talos Linux | linux | `talos linux`, `talos` | [linux/talos-linux/selfhst-sidero-talos.svg](linux/talos-linux/selfhst-sidero-talos.svg) |
| `trisquel` | Trisquel GNU/Linux | linux | `trisquel gnu linux`, `trisquel` | [linux/trisquel/fontlogos-trisquel.svg](linux/trisquel/fontlogos-trisquel.svg) |
| `turnkey-linux` | TurnKey Linux | linux | `turnkey linux`, `turnkey` | [linux/turnkey-linux/selfhst-turnkey-linux.svg](linux/turnkey-linux/selfhst-turnkey-linux.svg) |
| `tuxedo-os` | TUXEDO OS | linux | `tuxedo os`, `tuxedo` | [linux/tuxedo-os/papirus-tuxedo-os.svg](linux/tuxedo-os/papirus-tuxedo-os.svg) |
| `ubuntu` | Ubuntu | linux | `ubuntu` | [linux/ubuntu/selfhst-ubuntu.svg](linux/ubuntu/selfhst-ubuntu.svg) |
| `ubuntu-budgie` | Ubuntu Budgie | linux | `ubuntu budgie` | [linux/ubuntu-budgie/papirus-ubuntu-budgie.svg](linux/ubuntu-budgie/papirus-ubuntu-budgie.svg) |
| `ubuntu-mate` | Ubuntu MATE | linux | `ubuntu mate` | [linux/ubuntu-mate/thesvg-color-ubuntu-mate.svg](linux/ubuntu-mate/thesvg-color-ubuntu-mate.svg) |
| `ubuntu-studio` | Ubuntu Studio | linux | `ubuntu studio` | [linux/ubuntu-studio/papirus-ubuntu-studio.svg](linux/ubuntu-studio/papirus-ubuntu-studio.svg) |
| `ultimate-edition` | Ultimate Edition | linux | `ultimate edition` | [linux/ultimate-edition/papirus-ultimate-edition.svg](linux/ultimate-edition/papirus-ultimate-edition.svg) |
| `uos` | UnionTech UOS | linux | `uniontech uos`, `uniontech os`, `uos` | [linux/uos/official-uos.png](linux/uos/official-uos.png) |
| `vanilla-os` | Vanilla OS | linux | `vanilla os`, `vanilla` | [linux/vanilla-os/papirus-vanilla-os.svg](linux/vanilla-os/papirus-vanilla-os.svg) |
| `void-linux` | Void Linux | linux | `void linux`, `void` | [linux/void-linux/thesvg-color-void-linux.svg](linux/void-linux/thesvg-color-void-linux.svg) |
| `voyager` | Voyager Live | linux | `voyager live`, `voyager` | [linux/voyager/papirus-voyager.svg](linux/voyager/papirus-voyager.svg) |
| `xerolinux` | XeroLinux | linux | `xerolinux` | [linux/xerolinux/fontlogos-xerolinux.svg](linux/xerolinux/fontlogos-xerolinux.svg) |
| `xubuntu` | Xubuntu | linux | `xubuntu` | [linux/xubuntu/selfhst-xubuntu.svg](linux/xubuntu/selfhst-xubuntu.svg) |
| `zorin-os` | Zorin OS | linux | `zorin os`, `zorin` | [linux/zorin-os/selfhst-zorin-os.svg](linux/zorin-os/selfhst-zorin-os.svg) |
| `macos` | macOS | macos | `macos`, `mac os`, `darwin` | [macos/selfhst-apple-retro.svg](macos/selfhst-apple-retro.svg) |
| `windows` | Windows | windows | `windows`, `microsoft` | [windows/selfhst-microsoft-windows.svg](windows/selfhst-microsoft-windows.svg) |
